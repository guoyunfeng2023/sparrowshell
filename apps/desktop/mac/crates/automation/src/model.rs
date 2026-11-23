use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sparrowshell_domain::TargetRef;
use sparrowshell_execution::{CommandSource, ExecutionId, ExecutionMode, ExecutionState};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

pub const MAX_BATCH_TARGETS: usize = 64;
pub const MAX_WORKFLOW_STEPS: usize = 64;
pub const MAX_WORKFLOW_TRANSITIONS: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchSpec {
    pub command: String,
    pub targets: Vec<TargetRef>,
    pub max_concurrent: usize,
    pub mode: ExecutionMode,
}

impl BatchSpec {
    pub fn validate(&self) -> Result<(), AutomationError> {
        validate_command(&self.command)?;
        if self.targets.is_empty() || self.targets.len() > MAX_BATCH_TARGETS {
            return Err(AutomationError::InvalidTargetCount);
        }
        if self.max_concurrent == 0 || self.max_concurrent > 16 {
            return Err(AutomationError::InvalidConcurrency);
        }
        let unique: HashSet<_> = self
            .targets
            .iter()
            .map(|target| (target.server_id, target.session_id, target.generation))
            .collect();
        if unique.len() != self.targets.len() {
            return Err(AutomationError::DuplicateTarget);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Queued,
    Running,
    Succeeded,
    PartiallySucceeded,
    Failed,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRunState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRun {
    pub target: TargetRef,
    pub state: TargetRunState,
    pub execution_id: Option<ExecutionId>,
    pub exit_status: Option<u32>,
    pub error_code: Option<String>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRun {
    pub id: Uuid,
    pub spec: BatchSpec,
    pub state: BatchState,
    pub targets: Vec<TargetRun>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl BatchRun {
    pub fn new(spec: BatchSpec) -> Result<Self, AutomationError> {
        spec.validate()?;
        let targets = spec
            .targets
            .iter()
            .copied()
            .map(|target| TargetRun {
                target,
                state: TargetRunState::Queued,
                execution_id: None,
                exit_status: None,
                error_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })
            .collect();
        Ok(Self {
            id: Uuid::new_v4(),
            spec,
            state: BatchState::Queued,
            targets,
            created_at: Utc::now(),
            finished_at: None,
        })
    }

    /// Recovery never pretends a remote command stopped or succeeded. Durable
    /// integration must call this for unfinished runs after a process restart.
    pub fn mark_interrupted_after_restart(&mut self) {
        if !matches!(self.state, BatchState::Queued | BatchState::Running) {
            return;
        }
        for target in &mut self.targets {
            if matches!(
                target.state,
                TargetRunState::Queued | TargetRunState::Running
            ) {
                target.state = TargetRunState::Unknown;
                target.error_code = Some("interrupted_result_unknown".into());
            }
        }
        self.state = BatchState::Unknown;
        self.finished_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowVersion {
    pub definition_id: Uuid,
    pub version: u64,
    pub start_step: String,
    pub steps: Vec<WorkflowStep>,
    pub max_transitions: u32,
    pub created_at: DateTime<Utc>,
    pub content_hash: String,
}

impl WorkflowVersion {
    pub fn publish(
        definition_id: Uuid,
        version: u64,
        start_step: String,
        steps: Vec<WorkflowStep>,
        max_transitions: u32,
    ) -> Result<Self, AutomationError> {
        if version == 0 || max_transitions == 0 || max_transitions > MAX_WORKFLOW_TRANSITIONS {
            return Err(AutomationError::InvalidWorkflowLimit);
        }
        validate_steps(&start_step, &steps)?;
        Self {
            definition_id,
            version,
            start_step,
            steps,
            max_transitions,
            created_at: Utc::now(),
            content_hash: String::new(),
        }
        .with_canonical_hash()
    }

    pub fn revalidated(self) -> Result<Self, AutomationError> {
        let _ = Self::publish(
            self.definition_id,
            self.version,
            self.start_step.clone(),
            self.steps.clone(),
            self.max_transitions,
        )?;
        self.with_canonical_hash()
    }

    fn with_canonical_hash(mut self) -> Result<Self, AutomationError> {
        self.content_hash.clear();
        let encoded = serde_json::to_vec(&self).map_err(|_| AutomationError::InvalidWorkflow)?;
        self.content_hash = format!("{:x}", Sha256::digest(encoded));
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: String,
    pub kind: WorkflowStepKind,
    pub on_success: Option<String>,
    pub on_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "config")]
pub enum WorkflowStepKind {
    Command(CommandStep),
    Wait(WaitStep),
    Condition(ConditionExpression),
    End,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandStep {
    pub target: TargetRef,
    pub command: String,
    pub source: CommandSource,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub retry_delay_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitStep {
    pub seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "field")]
pub enum ConditionExpression {
    ExitCode { comparison: Comparison, value: i64 },
    StdoutContains { value: String },
    StderrContains { value: String },
    ExecutionState { value: ExecutionState },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Eq,
    NotEq,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}

impl Comparison {
    pub fn matches(self, actual: i64, expected: i64) -> bool {
        match self {
            Self::Eq => actual == expected,
            Self::NotEq => actual != expected,
            Self::LessThan => actual < expected,
            Self::LessOrEqual => actual <= expected,
            Self::GreaterThan => actual > expected,
            Self::GreaterOrEqual => actual >= expected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunState {
    Running,
    AwaitingApproval,
    Succeeded,
    Failed,
    Stopped,
    TimedOut,
    ResultUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepResult {
    pub step_id: String,
    pub state: ExecutionState,
    pub execution_id: Option<ExecutionId>,
    pub exit_status: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub version: u64,
    pub version_hash: String,
    pub state: WorkflowRunState,
    pub steps: Vec<WorkflowStepResult>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl WorkflowRun {
    pub fn mark_interrupted_after_restart(&mut self) {
        if !matches!(
            self.state,
            WorkflowRunState::Running | WorkflowRunState::AwaitingApproval
        ) {
            return;
        }
        for step in &mut self.steps {
            if matches!(
                step.state,
                ExecutionState::AwaitingApproval
                    | ExecutionState::Authorized
                    | ExecutionState::Running
            ) {
                step.state = ExecutionState::Unknown;
                step.error_code = Some("interrupted_result_unknown".into());
            }
        }
        self.state = WorkflowRunState::ResultUnknown;
        self.finished_at = Some(Utc::now());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AutomationError {
    #[error("command is empty, too long or contains NUL")]
    InvalidCommand,
    #[error("batch target count must be between 1 and 64")]
    InvalidTargetCount,
    #[error("batch concurrency must be between 1 and 16")]
    InvalidConcurrency,
    #[error("batch targets must be unique")]
    DuplicateTarget,
    #[error("workflow limits are invalid")]
    InvalidWorkflowLimit,
    #[error("workflow definition or graph is invalid")]
    InvalidWorkflow,
    #[error("workflow version already exists and is immutable")]
    VersionConflict,
    #[error("execution rejected the requested target or command")]
    ExecutionRejected,
    #[error("automation state is unavailable")]
    StateUnavailable,
}

pub fn validate_command(command: &str) -> Result<(), AutomationError> {
    if command.trim().is_empty() || command.len() > 16_384 || command.contains('\0') {
        Err(AutomationError::InvalidCommand)
    } else {
        Ok(())
    }
}

fn validate_steps(start: &str, steps: &[WorkflowStep]) -> Result<(), AutomationError> {
    if steps.is_empty() || steps.len() > MAX_WORKFLOW_STEPS {
        return Err(AutomationError::InvalidWorkflow);
    }
    let mut by_id = HashMap::new();
    for step in steps {
        if step.id.is_empty()
            || step.id.len() > 64
            || !step
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            || by_id.insert(step.id.as_str(), step).is_some()
        {
            return Err(AutomationError::InvalidWorkflow);
        }
        match &step.kind {
            WorkflowStepKind::Command(command) => {
                validate_command(&command.command)?;
                if command.timeout_seconds == 0
                    || command.timeout_seconds > 300
                    || command.retries > 5
                    || command.retry_delay_seconds > 60
                    || !matches!(command.source, CommandSource::Workflow | CommandSource::Ai)
                {
                    return Err(AutomationError::InvalidWorkflow);
                }
            }
            WorkflowStepKind::Wait(wait) if wait.seconds == 0 || wait.seconds > 300 => {
                return Err(AutomationError::InvalidWorkflow)
            }
            WorkflowStepKind::Condition(condition) => match condition {
                ConditionExpression::StdoutContains { value }
                | ConditionExpression::StderrContains { value }
                    if value.is_empty() || value.len() > 256 =>
                {
                    return Err(AutomationError::InvalidWorkflow)
                }
                _ => {}
            },
            _ => {}
        }
    }
    if !by_id.contains_key(start) {
        return Err(AutomationError::InvalidWorkflow);
    }
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([start]);
    let mut can_finish = false;
    while let Some(id) = queue.pop_front() {
        if !reachable.insert(id) {
            continue;
        }
        let step = by_id.get(id).ok_or(AutomationError::InvalidWorkflow)?;
        if matches!(step.kind, WorkflowStepKind::End | WorkflowStepKind::Failure) {
            if step.on_success.is_some() || step.on_failure.is_some() {
                return Err(AutomationError::InvalidWorkflow);
            }
            can_finish = true;
        }
        for next in [&step.on_success, &step.on_failure].into_iter().flatten() {
            if !by_id.contains_key(next.as_str()) {
                return Err(AutomationError::InvalidWorkflow);
            }
            queue.push_back(next);
        }
    }
    if reachable.len() != steps.len() || !can_finish {
        return Err(AutomationError::InvalidWorkflow);
    }
    Ok(())
}
