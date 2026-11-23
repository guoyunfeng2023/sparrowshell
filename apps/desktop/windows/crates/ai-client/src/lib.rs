//! Multi-step AI orchestration. This crate can only call the ExecutionBroker;
//! it has no SSH transport, credential or terminal-channel access.

pub mod billing;
pub mod context;
pub mod gateway;

use serde::{Deserialize, Serialize};
use sparrowshell_domain::TargetRef;
use sparrowshell_execution::{
    ApprovalId, CommandHash, CommandProposal, CommandRequest, CommandSource, ExecutionBroker,
    ExecutionDecision, ExecutionError, ExecutionId, ExecutionMode, ExecutionPermit,
    ExecutionRecord, ExecutionState, PendingApproval,
};
use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanId(pub Uuid);

impl PlanId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for PlanId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiPlanState {
    Ready,
    Running,
    AwaitingApproval,
    Stopping,
    PausedDisconnected,
    Stopped,
    Completed,
    Failed,
    ResultUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiStepState {
    Queued,
    AwaitingApproval,
    Running,
    StopRequested,
    PausedDisconnected,
    Succeeded,
    Failed,
    Stopped,
    Unknown,
    UnknownAfterDisconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStep {
    pub proposal: CommandProposal,
    pub state: AiStepState,
    pub execution_id: Option<ExecutionId>,
    pub approval_id: Option<ApprovalId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlan {
    pub plan_id: PlanId,
    pub target: TargetRef,
    pub state: AiPlanState,
    pub steps: Vec<AiStep>,
    pub current_step: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepProgress {
    pub plan: AiPlan,
    pub approval: Option<PendingApproval>,
    pub execution: Option<ExecutionRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("AI plan does not exist")]
    PlanNotFound,
    #[error("AI plan must contain at least one step")]
    EmptyPlan,
    #[error("AI plan is not runnable in its current state")]
    PlanNotRunnable,
    #[error("AI plan target cannot drift to another server, session or generation")]
    TargetChanged,
    #[error("a command has an unknown result and requires manual recovery")]
    ManualRecoveryRequired,
    #[error("approval does not belong to the current plan step")]
    ApprovalStepMismatch,
    #[error("AI plan state is unavailable")]
    StateUnavailable,
    #[error(transparent)]
    Execution(#[from] ExecutionError),
}

#[derive(Default)]
struct Plans {
    plans: HashMap<PlanId, AiPlan>,
}

#[derive(Clone)]
pub struct AiOrchestrator {
    broker: ExecutionBroker,
    plans: Arc<Mutex<Plans>>,
}

impl AiOrchestrator {
    pub fn new(broker: ExecutionBroker) -> Self {
        Self {
            broker,
            plans: Arc::new(Mutex::new(Plans::default())),
        }
    }

    pub fn broker(&self) -> &ExecutionBroker {
        &self.broker
    }

    pub fn create_plan(
        &self,
        target: TargetRef,
        proposals: Vec<CommandProposal>,
    ) -> Result<AiPlan, AiError> {
        if proposals.is_empty() {
            return Err(AiError::EmptyPlan);
        }
        let plan = AiPlan {
            plan_id: PlanId::new(),
            target,
            state: AiPlanState::Ready,
            steps: proposals
                .into_iter()
                .map(|proposal| AiStep {
                    proposal,
                    state: AiStepState::Queued,
                    execution_id: None,
                    approval_id: None,
                })
                .collect(),
            current_step: Some(0),
        };
        self.lock()?.plans.insert(plan.plan_id, plan.clone());
        Ok(plan)
    }

    pub fn plan(&self, plan_id: PlanId) -> Option<AiPlan> {
        self.plans.lock().ok()?.plans.get(&plan_id).cloned()
    }

    /// Executes at most one step. The caller supplies the current local target
    /// and mode on every invocation, so neither can be supplied by the model.
    pub async fn run_next<E: sparrowshell_execution::CommandExecutor + ?Sized>(
        &self,
        plan_id: PlanId,
        current_target: TargetRef,
        mode: ExecutionMode,
        executor: &E,
    ) -> Result<StepProgress, AiError> {
        let (index, proposal) = {
            let mut plans = self.lock()?;
            let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
            if plan.state != AiPlanState::Ready {
                return Err(AiError::PlanNotRunnable);
            }
            if plan.target != current_target {
                return Err(AiError::TargetChanged);
            }
            let index = plan.current_step.ok_or(AiError::PlanNotRunnable)?;
            let step = plan.steps.get(index).ok_or(AiError::PlanNotRunnable)?;
            if step.state != AiStepState::Queued {
                return Err(AiError::PlanNotRunnable);
            }
            (index, step.proposal.clone())
        };
        let decision = match self.broker.submit(
            CommandRequest {
                source: CommandSource::Ai,
                target: current_target,
                proposal,
            },
            mode,
        ) {
            Ok(decision) => decision,
            Err(error) => return Err(error.into()),
        };
        match decision {
            ExecutionDecision::RequiresApproval(approval) => {
                let plan = {
                    let mut plans = self.lock()?;
                    let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
                    plan.state = AiPlanState::AwaitingApproval;
                    let step = &mut plan.steps[index];
                    step.state = AiStepState::AwaitingApproval;
                    step.execution_id = Some(approval.execution_id);
                    step.approval_id = Some(approval.approval_id);
                    plan.clone()
                };
                Ok(StepProgress {
                    plan,
                    approval: Some(approval),
                    execution: None,
                })
            }
            ExecutionDecision::Execute(permit) => {
                self.begin_execution(plan_id, index, &permit)?;
                self.execute_permit(plan_id, index, permit, executor).await
            }
        }
    }

    pub async fn approve_and_run<E: sparrowshell_execution::CommandExecutor + ?Sized>(
        &self,
        plan_id: PlanId,
        approval_id: ApprovalId,
        command_hash: &CommandHash,
        current_target: TargetRef,
        executor: &E,
    ) -> Result<StepProgress, AiError> {
        let index = {
            let plans = self.lock()?;
            let plan = plans.plans.get(&plan_id).ok_or(AiError::PlanNotFound)?;
            if plan.state != AiPlanState::AwaitingApproval {
                return Err(AiError::PlanNotRunnable);
            }
            if plan.target != current_target {
                return Err(AiError::TargetChanged);
            }
            let index = plan.current_step.ok_or(AiError::PlanNotRunnable)?;
            let step = &plan.steps[index];
            if step.state != AiStepState::AwaitingApproval || step.approval_id != Some(approval_id)
            {
                return Err(AiError::ApprovalStepMismatch);
            }
            index
        };
        let permit = self.broker.approve(
            approval_id,
            command_hash,
            current_target,
            chrono::Utc::now(),
        )?;
        self.begin_execution(plan_id, index, &permit)?;
        self.execute_permit(plan_id, index, permit, executor).await
    }

    /// Cancels all work that has not started. A running command only becomes
    /// StopRequested until the real channel reports cancellation or a result.
    pub fn stop(&self, plan_id: PlanId) -> Result<AiPlan, AiError> {
        let mut plans = self.lock()?;
        let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
        let mut running = false;
        for step in &mut plan.steps {
            match step.state {
                AiStepState::Queued
                | AiStepState::AwaitingApproval
                | AiStepState::PausedDisconnected => {
                    if let Some(execution_id) = step.execution_id {
                        let _ = self.broker.request_stop(execution_id);
                    }
                    step.state = AiStepState::Stopped;
                    step.approval_id = None;
                }
                AiStepState::Running | AiStepState::StopRequested => {
                    if let Some(execution_id) = step.execution_id {
                        let record = self.broker.request_stop(execution_id)?;
                        if record.state == ExecutionState::StopRequested {
                            step.state = AiStepState::StopRequested;
                            running = true;
                        } else if record.state == ExecutionState::Stopped {
                            step.state = AiStepState::Stopped;
                        }
                    }
                }
                _ => {}
            }
        }
        plan.state = if running {
            AiPlanState::Stopping
        } else {
            AiPlanState::Stopped
        };
        Ok(plan.clone())
    }

    pub fn disconnected(&self, target: TargetRef) -> Result<(), AiError> {
        self.broker.disconnect(target)?;
        let mut plans = self.lock()?;
        for plan in plans.plans.values_mut() {
            if plan.target != target
                || matches!(
                    plan.state,
                    AiPlanState::Stopped | AiPlanState::Completed | AiPlanState::Failed
                )
            {
                continue;
            }
            if let Some(index) = plan.current_step {
                let step = &mut plan.steps[index];
                step.state = match step.state {
                    AiStepState::Running | AiStepState::StopRequested => {
                        AiStepState::UnknownAfterDisconnect
                    }
                    AiStepState::AwaitingApproval | AiStepState::Queued => {
                        AiStepState::PausedDisconnected
                    }
                    state => state,
                };
            }
            plan.state = AiPlanState::PausedDisconnected;
        }
        Ok(())
    }

    /// Reconnection never calls this automatically. A queued plan can be
    /// explicitly rebound to a newer generation; an unknown running result
    /// requires manual inspection instead of a potentially duplicated rerun.
    pub fn resume(&self, plan_id: PlanId, current_target: TargetRef) -> Result<AiPlan, AiError> {
        let mut plans = self.lock()?;
        let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
        if plan.state != AiPlanState::PausedDisconnected {
            return Err(AiError::PlanNotRunnable);
        }
        if plan.target.server_id != current_target.server_id
            || plan.target.session_id != current_target.session_id
            || current_target.generation <= plan.target.generation
        {
            return Err(AiError::TargetChanged);
        }
        if let Some(index) = plan.current_step {
            let step = &mut plan.steps[index];
            if matches!(
                step.state,
                AiStepState::Unknown | AiStepState::UnknownAfterDisconnect
            ) {
                return Err(AiError::ManualRecoveryRequired);
            }
            if step.state == AiStepState::PausedDisconnected {
                step.state = AiStepState::Queued;
                step.execution_id = None;
                step.approval_id = None;
            }
        }
        plan.target = current_target;
        plan.state = AiPlanState::Ready;
        Ok(plan.clone())
    }

    fn begin_execution(
        &self,
        plan_id: PlanId,
        index: usize,
        permit: &ExecutionPermit,
    ) -> Result<(), AiError> {
        let mut plans = self.lock()?;
        let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
        plan.state = AiPlanState::Running;
        let step = &mut plan.steps[index];
        step.state = AiStepState::Running;
        step.execution_id = Some(permit.execution_id);
        step.approval_id = None;
        Ok(())
    }

    async fn execute_permit<E: sparrowshell_execution::CommandExecutor + ?Sized>(
        &self,
        plan_id: PlanId,
        index: usize,
        permit: ExecutionPermit,
        executor: &E,
    ) -> Result<StepProgress, AiError> {
        let execution = self.broker.execute(permit, executor).await?;
        let plan = {
            let mut plans = self.lock()?;
            let plan = plans.plans.get_mut(&plan_id).ok_or(AiError::PlanNotFound)?;
            let was_stopping = plan.state == AiPlanState::Stopping;
            plan.steps[index].state = step_state(execution.state);
            plan.state =
                match execution.state {
                    ExecutionState::Succeeded if was_stopping => AiPlanState::Stopped,
                    ExecutionState::Succeeded => {
                        if let Some(next) = plan.steps.iter().enumerate().skip(index + 1).find_map(
                            |(index, step)| (step.state == AiStepState::Queued).then_some(index),
                        ) {
                            plan.current_step = Some(next);
                            AiPlanState::Ready
                        } else if plan
                            .steps
                            .iter()
                            .all(|step| step.state == AiStepState::Succeeded)
                        {
                            plan.current_step = None;
                            AiPlanState::Completed
                        } else {
                            AiPlanState::Stopped
                        }
                    }
                    ExecutionState::Failed => AiPlanState::Failed,
                    ExecutionState::Stopped => AiPlanState::Stopped,
                    ExecutionState::UnknownAfterDisconnect | ExecutionState::PausedDisconnected => {
                        AiPlanState::PausedDisconnected
                    }
                    ExecutionState::Unknown => AiPlanState::ResultUnknown,
                    _ => AiPlanState::ResultUnknown,
                };
            plan.clone()
        };
        Ok(StepProgress {
            plan,
            approval: None,
            execution: Some(execution),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Plans>, AiError> {
        self.plans.lock().map_err(|_| AiError::StateUnavailable)
    }
}

fn step_state(state: ExecutionState) -> AiStepState {
    match state {
        ExecutionState::AwaitingApproval => AiStepState::AwaitingApproval,
        ExecutionState::Authorized => AiStepState::Queued,
        ExecutionState::Running => AiStepState::Running,
        ExecutionState::StopRequested => AiStepState::StopRequested,
        ExecutionState::Succeeded => AiStepState::Succeeded,
        ExecutionState::Failed => AiStepState::Failed,
        ExecutionState::Stopped => AiStepState::Stopped,
        ExecutionState::PausedDisconnected => AiStepState::PausedDisconnected,
        ExecutionState::Unknown => AiStepState::Unknown,
        ExecutionState::UnknownAfterDisconnect => AiStepState::UnknownAfterDisconnect,
        ExecutionState::Expired => AiStepState::Stopped,
    }
}
