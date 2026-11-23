use crate::{
    AutomationError, ConditionExpression, WorkflowDefinition, WorkflowRun, WorkflowRunState,
    WorkflowStepKind, WorkflowStepResult, WorkflowVersion,
};
use sparrowshell_execution::{
    CommandExecutor, CommandProposal, CommandRequest, ExecutionBroker, ExecutionDecision,
    ExecutionMode, ExecutionRecord, ExecutionState, PendingApproval,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use uuid::Uuid;

const MAX_TOTAL_SECONDS: u64 = 900;

#[derive(Clone, Default)]
pub struct WorkflowCatalog {
    definitions: Arc<Mutex<HashMap<Uuid, WorkflowDefinition>>>,
    versions: Arc<Mutex<HashMap<(Uuid, u64), WorkflowVersion>>>,
}

impl WorkflowCatalog {
    pub fn create_definition(
        &self,
        name: impl Into<String>,
    ) -> Result<WorkflowDefinition, AutomationError> {
        let name = name.into();
        if name.trim().is_empty() || name.len() > 128 {
            return Err(AutomationError::InvalidWorkflow);
        }
        let definition = WorkflowDefinition {
            id: Uuid::new_v4(),
            name: name.trim().into(),
            created_at: chrono::Utc::now(),
        };
        self.definitions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .insert(definition.id, definition.clone());
        Ok(definition)
    }

    pub fn publish(&self, version: WorkflowVersion) -> Result<WorkflowVersion, AutomationError> {
        if !self
            .definitions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .contains_key(&version.definition_id)
        {
            return Err(AutomationError::InvalidWorkflow);
        }
        let mut versions = self
            .versions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        if versions.contains_key(&(version.definition_id, version.version)) {
            return Err(AutomationError::VersionConflict);
        }
        versions.insert((version.definition_id, version.version), version.clone());
        Ok(version)
    }

    pub fn version(&self, definition: Uuid, version: u64) -> Option<WorkflowVersion> {
        self.versions
            .lock()
            .ok()?
            .get(&(definition, version))
            .cloned()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecution {
    pub run: WorkflowRun,
    pub pending_approval: Option<PendingApproval>,
}

pub struct WorkflowRunContext<'a, E: CommandExecutor + ?Sized> {
    pub broker: &'a ExecutionBroker,
    pub executor: &'a E,
    pub mode: ExecutionMode,
    pub cancel: watch::Receiver<bool>,
}

#[derive(Clone, Default)]
pub struct WorkflowEngine {
    runs: Arc<Mutex<HashMap<Uuid, WorkflowRun>>>,
    versions: Arc<Mutex<HashMap<Uuid, WorkflowVersion>>>,
    pending_steps: Arc<Mutex<HashMap<Uuid, String>>>,
}

impl WorkflowEngine {
    pub fn start(
        &self,
        version: WorkflowVersion,
        _mode: ExecutionMode,
    ) -> Result<Uuid, AutomationError> {
        // Revalidate deserialized definitions and use the canonical hash for history.
        let checked = version.revalidated()?;
        let id = Uuid::new_v4();
        let run = WorkflowRun {
            id,
            definition_id: checked.definition_id,
            version: checked.version,
            version_hash: checked.content_hash.clone(),
            state: WorkflowRunState::Running,
            steps: Vec::new(),
            started_at: chrono::Utc::now(),
            finished_at: None,
        };
        self.versions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .insert(id, checked);
        self.runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .insert(id, run);
        Ok(id)
    }

    pub fn snapshot(&self, id: Uuid) -> Option<WorkflowRun> {
        self.runs.lock().ok()?.get(&id).cloned()
    }

    pub fn recover_interrupted(&self, id: Uuid) -> Result<WorkflowRun, AutomationError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        let run = runs.get_mut(&id).ok_or(AutomationError::StateUnavailable)?;
        run.mark_interrupted_after_restart();
        Ok(run.clone())
    }

    pub fn stop_awaiting(&self, id: Uuid) -> Result<WorkflowExecution, AutomationError> {
        if self.snapshot(id).map(|run| run.state) != Some(WorkflowRunState::AwaitingApproval) {
            return Err(AutomationError::StateUnavailable);
        }
        self.finish(id, WorkflowRunState::Stopped, None)
    }

    pub async fn run<E: CommandExecutor + ?Sized>(
        &self,
        id: Uuid,
        broker: &ExecutionBroker,
        executor: &E,
        mode: ExecutionMode,
        cancel: watch::Receiver<bool>,
    ) -> Result<WorkflowExecution, AutomationError> {
        let start = self
            .versions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .get(&id)
            .cloned()
            .ok_or(AutomationError::StateUnavailable)?
            .start_step;
        self.run_from(
            id,
            &start,
            None,
            WorkflowRunContext {
                broker,
                executor,
                mode,
                cancel,
            },
        )
        .await
    }

    pub async fn approve_and_run<E: CommandExecutor + ?Sized>(
        &self,
        id: Uuid,
        approval_id: sparrowshell_execution::ApprovalId,
        command_hash: sparrowshell_execution::CommandHash,
        context: WorkflowRunContext<'_, E>,
    ) -> Result<WorkflowExecution, AutomationError> {
        if self.snapshot(id).map(|run| run.state) != Some(WorkflowRunState::AwaitingApproval) {
            return Err(AutomationError::StateUnavailable);
        }
        let step = self
            .pending_steps
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .get(&id)
            .cloned()
            .ok_or(AutomationError::StateUnavailable)?;
        self.run_from(id, &step, Some((approval_id, command_hash)), context)
            .await
    }

    async fn run_from<E: CommandExecutor + ?Sized>(
        &self,
        id: Uuid,
        start_step: &str,
        mut approved: Option<(
            sparrowshell_execution::ApprovalId,
            sparrowshell_execution::CommandHash,
        )>,
        context: WorkflowRunContext<'_, E>,
    ) -> Result<WorkflowExecution, AutomationError> {
        let WorkflowRunContext {
            broker,
            executor,
            mode,
            mut cancel,
        } = context;
        let version = self
            .versions
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .get(&id)
            .cloned()
            .ok_or(AutomationError::StateUnavailable)?;
        let steps: HashMap<_, _> = version
            .steps
            .iter()
            .map(|step| (step.id.as_str(), step))
            .collect();
        let mut current = start_step;
        let mut last_command: Option<ExecutionRecord> = None;
        let deadline = Instant::now() + Duration::from_secs(MAX_TOTAL_SECONDS);

        for _ in 0..version.max_transitions {
            if *cancel.borrow() {
                return self.finish(id, WorkflowRunState::Stopped, None);
            }
            if Instant::now() >= deadline {
                return self.finish(id, WorkflowRunState::TimedOut, None);
            }
            let step = steps.get(current).ok_or(AutomationError::InvalidWorkflow)?;
            match &step.kind {
                WorkflowStepKind::End => return self.finish(id, WorkflowRunState::Succeeded, None),
                WorkflowStepKind::Failure => {
                    return self.finish(id, WorkflowRunState::Failed, None)
                }
                WorkflowStepKind::Wait(wait) => {
                    let sleep = tokio::time::sleep(Duration::from_secs(wait.seconds));
                    tokio::pin!(sleep);
                    tokio::select! {
                        _ = &mut sleep => {}
                        _ = cancel.changed() => return self.finish(id, WorkflowRunState::Stopped, None),
                        _ = tokio::time::sleep_until(deadline.into()) => {
                            return self.finish(id, WorkflowRunState::TimedOut, None)
                        }
                    }
                    current = next(step.on_success.as_deref())?;
                }
                WorkflowStepKind::Condition(condition) => {
                    let result = last_command
                        .as_ref()
                        .ok_or(AutomationError::InvalidWorkflow)?;
                    let matched = evaluate(condition, result);
                    current = next(if matched {
                        step.on_success.as_deref()
                    } else {
                        step.on_failure.as_deref()
                    })?;
                }
                WorkflowStepKind::Command(command) => {
                    let mut attempt = 0;
                    let record = loop {
                        if *cancel.borrow() {
                            return self.finish(id, WorkflowRunState::Stopped, None);
                        }
                        let request = CommandRequest {
                            source: command.source,
                            target: command.target,
                            proposal: CommandProposal {
                                command: command.command.clone(),
                                model_risk: None,
                            },
                        };
                        let permit = if let Some((approval_id, command_hash)) = approved.take() {
                            match broker.approve(
                                approval_id,
                                &command_hash,
                                command.target,
                                chrono::Utc::now(),
                            ) {
                                Ok(permit) => permit,
                                Err(_) => return self.finish(id, WorkflowRunState::Failed, None),
                            }
                        } else {
                            match broker.submit(request, mode) {
                                Ok(ExecutionDecision::Execute(permit)) => permit,
                                Ok(ExecutionDecision::RequiresApproval(approval)) => {
                                    self.push_result(id, step_result(&step.id, None))?;
                                    self.pending_steps
                                        .lock()
                                        .map_err(|_| AutomationError::StateUnavailable)?
                                        .insert(id, step.id.clone());
                                    return self.finish(
                                        id,
                                        WorkflowRunState::AwaitingApproval,
                                        Some(approval),
                                    );
                                }
                                Err(_) => return self.finish(id, WorkflowRunState::Failed, None),
                            }
                        };
                        let execution_id = permit.execution_id;
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        let timeout = Duration::from_secs(command.timeout_seconds).min(remaining);
                        let execution = broker.execute(permit, executor);
                        tokio::pin!(execution);
                        let result = tokio::select! {
                            result = tokio::time::timeout(timeout, &mut execution) => {
                                match result {
                                    Ok(result) => result,
                                    Err(_) => {
                                        let _ = broker.request_stop(execution_id);
                                        return self.finish(id, WorkflowRunState::TimedOut, None);
                                    }
                                }
                            }
                            _ = cancel.changed() => {
                                let _ = broker.request_stop(execution_id);
                                match tokio::time::timeout(timeout, &mut execution).await {
                                    Ok(result) => result,
                                    Err(_) => return self.finish(id, WorkflowRunState::Stopped, None),
                                }
                            }
                        };
                        let record = match result {
                            Ok(record) => record,
                            Err(_) => return self.finish(id, WorkflowRunState::Failed, None),
                        };
                        self.push_result(id, step_result(&step.id, Some(&record)))?;
                        let retryable =
                            record.state == ExecutionState::Failed && attempt < command.retries;
                        if !retryable {
                            break record;
                        }
                        attempt += 1;
                        tokio::time::sleep(Duration::from_secs(command.retry_delay_seconds)).await;
                    };
                    if matches!(
                        record.state,
                        ExecutionState::Unknown | ExecutionState::UnknownAfterDisconnect
                    ) {
                        return self.finish(id, WorkflowRunState::ResultUnknown, None);
                    }
                    if record.state == ExecutionState::Stopped {
                        return self.finish(id, WorkflowRunState::Stopped, None);
                    }
                    let succeeded = record.state == ExecutionState::Succeeded;
                    current = next(if succeeded {
                        step.on_success.as_deref()
                    } else {
                        step.on_failure.as_deref()
                    })?;
                    last_command = Some(record);
                }
            }
        }
        self.finish(id, WorkflowRunState::TimedOut, None)
    }

    fn push_result(&self, id: Uuid, result: WorkflowStepResult) -> Result<(), AutomationError> {
        self.runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .get_mut(&id)
            .ok_or(AutomationError::StateUnavailable)?
            .steps
            .push(result);
        Ok(())
    }

    fn finish(
        &self,
        id: Uuid,
        state: WorkflowRunState,
        pending_approval: Option<PendingApproval>,
    ) -> Result<WorkflowExecution, AutomationError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        let run = runs.get_mut(&id).ok_or(AutomationError::StateUnavailable)?;
        run.state = state;
        if state != WorkflowRunState::AwaitingApproval {
            run.finished_at = Some(chrono::Utc::now());
            self.pending_steps
                .lock()
                .map_err(|_| AutomationError::StateUnavailable)?
                .remove(&id);
        }
        Ok(WorkflowExecution {
            run: run.clone(),
            pending_approval,
        })
    }
}

fn next(next: Option<&str>) -> Result<&str, AutomationError> {
    next.ok_or(AutomationError::InvalidWorkflow)
}

fn evaluate(condition: &ConditionExpression, record: &ExecutionRecord) -> bool {
    match condition {
        ConditionExpression::ExitCode { comparison, value } => record
            .exit_status
            .map(|actual| comparison.matches(i64::from(actual), *value))
            .unwrap_or(false),
        ConditionExpression::StdoutContains { value } => record.stdout.contains(value),
        ConditionExpression::StderrContains { value } => record.stderr.contains(value),
        ConditionExpression::ExecutionState { value } => record.state == *value,
    }
}

fn step_result(step_id: &str, record: Option<&ExecutionRecord>) -> WorkflowStepResult {
    WorkflowStepResult {
        step_id: step_id.into(),
        state: record
            .map(|record| record.state)
            .unwrap_or(ExecutionState::AwaitingApproval),
        execution_id: record.map(|record| record.execution_id),
        exit_status: record.and_then(|record| record.exit_status),
        stdout: record
            .map(|record| record.stdout.clone())
            .unwrap_or_default(),
        stderr: record
            .map(|record| record.stderr.clone())
            .unwrap_or_default(),
        error_code: record.and_then(|record| record.error_code.clone()),
    }
}
