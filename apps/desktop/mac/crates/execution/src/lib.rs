//! The sole authorization gate for AI, batch and workflow command execution.
//! Model output is only a proposal: target, mode and approval always come from
//! trusted local state. No SSH transport or credential type is visible here.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sparrowshell_domain::{SessionId, TargetRef};
use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const APPROVAL_TTL: Duration = Duration::minutes(5);

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

stable_id!(ExecutionId);
stable_id!(ApprovalId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandHash(String);

impl CommandHash {
    pub fn from_command(command: &str) -> Self {
        let digest = Sha256::digest(command.as_bytes());
        Self(format!("{digest:x}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    ConfirmEach,
    AutoExceptHighRisk,
    AutoAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Ai,
    Batch,
    Workflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Unknown,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandProposal {
    pub command: String,
    pub model_risk: Option<RiskLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    pub source: CommandSource,
    pub target: TargetRef,
    pub proposal: CommandProposal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub approval_id: ApprovalId,
    pub execution_id: ExecutionId,
    pub target: TargetRef,
    pub command: String,
    pub command_hash: CommandHash,
    pub risk: RiskAssessment,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPermit {
    pub execution_id: ExecutionId,
    pub target: TargetRef,
    pub command_hash: CommandHash,
    pub risk: RiskAssessment,
    pub mode: ExecutionMode,
    command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionDecision {
    Execute(ExecutionPermit),
    RequiresApproval(PendingApproval),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    AwaitingApproval,
    Authorized,
    Running,
    StopRequested,
    Succeeded,
    Failed,
    Stopped,
    PausedDisconnected,
    Unknown,
    UnknownAfterDisconnect,
    Expired,
}

impl ExecutionState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::Stopped
                | Self::PausedDisconnected
                | Self::Unknown
                | Self::UnknownAfterDisconnect
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub execution_id: ExecutionId,
    pub source: CommandSource,
    pub target: TargetRef,
    pub mode: ExecutionMode,
    pub command: String,
    pub command_hash: CommandHash,
    pub risk: RiskAssessment,
    pub state: ExecutionState,
    pub approval_id: Option<ApprovalId>,
    pub approval_expires_at: Option<DateTime<Utc>>,
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<u32>,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExecutorError {
    #[error("SSH disconnected before a trustworthy exit status arrived")]
    Disconnected,
    #[error("the remote exec channel confirmed cancellation")]
    Cancelled,
    #[error("the command result is unknown")]
    Unknown,
    #[error("the command transport failed")]
    TransportFailed,
}

#[async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn execute(
        &self,
        permit: &ExecutionPermit,
        command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionError {
    #[error("the target session is not active")]
    TargetNotActive,
    #[error("the target belongs to an old connection generation")]
    StaleTarget,
    #[error("approval does not exist or has already been consumed")]
    ApprovalNotFound,
    #[error("approval command hash or target does not match")]
    ApprovalMismatch,
    #[error("approval has expired")]
    ApprovalExpired,
    #[error("execution is not in an executable state")]
    InvalidExecutionState,
    #[error("execution does not exist")]
    ExecutionNotFound,
    #[error("execution state is unavailable")]
    StateUnavailable,
}

#[derive(Default)]
struct BrokerState {
    active_targets: HashMap<SessionId, TargetRef>,
    records: HashMap<ExecutionId, ExecutionRecord>,
}

#[derive(Clone, Default)]
pub struct ExecutionBroker {
    state: Arc<Mutex<BrokerState>>,
}

impl ExecutionBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a newly connected physical generation. Reconnecting makes all
    /// unstarted work on the previous generation pause and running work unknown.
    pub fn activate_target(&self, target: TargetRef) -> Result<(), ExecutionError> {
        let mut state = self.lock()?;
        if let Some(previous) = state.active_targets.get(&target.session_id).copied() {
            if previous == target {
                return Ok(());
            }
            if target.generation <= previous.generation {
                return Err(ExecutionError::StaleTarget);
            }
            invalidate_target(&mut state, previous);
        }
        state.active_targets.insert(target.session_id, target);
        Ok(())
    }

    pub fn disconnect(&self, target: TargetRef) -> Result<(), ExecutionError> {
        let mut state = self.lock()?;
        match state.active_targets.get(&target.session_id) {
            Some(active) if *active == target => {}
            Some(_) => return Err(ExecutionError::StaleTarget),
            None => return Err(ExecutionError::TargetNotActive),
        }
        state.active_targets.remove(&target.session_id);
        invalidate_target(&mut state, target);
        Ok(())
    }

    pub fn submit(
        &self,
        request: CommandRequest,
        mode: ExecutionMode,
    ) -> Result<ExecutionDecision, ExecutionError> {
        let mut state = self.lock()?;
        ensure_active(&state, request.target)?;
        let now = Utc::now();
        let execution_id = ExecutionId::new();
        let command_hash = CommandHash::from_command(&request.proposal.command);
        let risk = merge_model_risk(
            assess_risk(&request.proposal.command),
            request.proposal.model_risk,
        );
        let needs_approval = request.source == CommandSource::Ai
            && match mode {
                ExecutionMode::ConfirmEach => true,
                ExecutionMode::AutoExceptHighRisk => risk.level != RiskLevel::Low,
                ExecutionMode::AutoAll => false,
            };
        let approval_id = needs_approval.then(ApprovalId::new);
        let approval_expires_at = needs_approval.then(|| now + APPROVAL_TTL);
        let record = ExecutionRecord {
            execution_id,
            source: request.source,
            target: request.target,
            mode,
            command: request.proposal.command,
            command_hash: command_hash.clone(),
            risk: risk.clone(),
            state: if needs_approval {
                ExecutionState::AwaitingApproval
            } else {
                ExecutionState::Authorized
            },
            approval_id,
            approval_expires_at,
            stdout: String::new(),
            stderr: String::new(),
            exit_status: None,
            error_code: None,
            created_at: now,
            started_at: None,
            finished_at: None,
        };
        state.records.insert(execution_id, record.clone());
        if needs_approval {
            Ok(ExecutionDecision::RequiresApproval(PendingApproval {
                approval_id: approval_id.expect("approval id was created"),
                execution_id,
                target: request.target,
                command: record.command,
                command_hash,
                risk,
                expires_at: approval_expires_at.expect("approval deadline was created"),
            }))
        } else {
            Ok(ExecutionDecision::Execute(permit_from(&record)))
        }
    }

    pub fn approve(
        &self,
        approval_id: ApprovalId,
        command_hash: &CommandHash,
        target: TargetRef,
        now: DateTime<Utc>,
    ) -> Result<ExecutionPermit, ExecutionError> {
        let mut state = self.lock()?;
        let execution_id = state
            .records
            .iter()
            .find_map(|(id, record)| (record.approval_id == Some(approval_id)).then_some(*id))
            .ok_or(ExecutionError::ApprovalNotFound)?;
        {
            let record = state
                .records
                .get(&execution_id)
                .ok_or(ExecutionError::ApprovalNotFound)?;
            if record.state != ExecutionState::AwaitingApproval {
                return Err(ExecutionError::ApprovalNotFound);
            }
            if &record.command_hash != command_hash || record.target != target {
                return Err(ExecutionError::ApprovalMismatch);
            }
        }
        ensure_active(&state, target)?;
        let record = state
            .records
            .get_mut(&execution_id)
            .ok_or(ExecutionError::ApprovalNotFound)?;
        if record
            .approval_expires_at
            .is_some_and(|expires_at| now > expires_at)
        {
            record.state = ExecutionState::Expired;
            record.finished_at = Some(now);
            record.error_code = Some("approval_expired".into());
            return Err(ExecutionError::ApprovalExpired);
        }
        record.state = ExecutionState::Authorized;
        Ok(permit_from(record))
    }

    pub async fn execute<E: CommandExecutor + ?Sized>(
        &self,
        permit: ExecutionPermit,
        executor: &E,
    ) -> Result<ExecutionRecord, ExecutionError> {
        let command = {
            let mut state = self.lock()?;
            ensure_active(&state, permit.target)?;
            let record = state
                .records
                .get_mut(&permit.execution_id)
                .ok_or(ExecutionError::ExecutionNotFound)?;
            if record.state != ExecutionState::Authorized {
                return Err(ExecutionError::InvalidExecutionState);
            }
            if record.target != permit.target
                || record.command_hash != permit.command_hash
                || record.command != permit.command
                || record.mode != permit.mode
                || record.risk != permit.risk
            {
                return Err(ExecutionError::InvalidExecutionState);
            }
            record.state = ExecutionState::Running;
            record.started_at = Some(Utc::now());
            record.command.clone()
        };

        let result = executor.execute(&permit, &command).await;
        let mut state = self.lock()?;
        let record = state
            .records
            .get_mut(&permit.execution_id)
            .ok_or(ExecutionError::ExecutionNotFound)?;
        // A disconnect observed while awaiting the transport is authoritative:
        // a late channel result must not rewrite Unknown back to success.
        if record.state == ExecutionState::UnknownAfterDisconnect {
            return Ok(record.clone());
        }
        match result {
            Ok(output) => {
                record.stdout = output.stdout;
                record.stderr = output.stderr;
                record.exit_status = output.exit_status;
                record.state = match output.exit_status {
                    Some(0) => ExecutionState::Succeeded,
                    Some(_) => ExecutionState::Failed,
                    None => {
                        record.error_code = Some("missing_exit_status".into());
                        ExecutionState::Unknown
                    }
                };
            }
            Err(error) => {
                let (next, code) = match error {
                    ExecutorError::Disconnected => {
                        (ExecutionState::UnknownAfterDisconnect, "disconnected")
                    }
                    ExecutorError::Cancelled => (ExecutionState::Stopped, "cancelled"),
                    ExecutorError::Unknown => (ExecutionState::Unknown, "result_unknown"),
                    ExecutorError::TransportFailed => (ExecutionState::Failed, "transport_failed"),
                };
                record.state = next;
                record.error_code = Some(code.into());
            }
        }
        record.finished_at = Some(Utc::now());
        Ok(record.clone())
    }

    /// Stops queued work immediately. Running work only becomes StopRequested;
    /// its final state must come from the real exec-channel result.
    pub fn request_stop(
        &self,
        execution_id: ExecutionId,
    ) -> Result<ExecutionRecord, ExecutionError> {
        let mut state = self.lock()?;
        let record = state
            .records
            .get_mut(&execution_id)
            .ok_or(ExecutionError::ExecutionNotFound)?;
        record.state = match record.state {
            ExecutionState::AwaitingApproval | ExecutionState::Authorized => {
                ExecutionState::Stopped
            }
            ExecutionState::Running => ExecutionState::StopRequested,
            state => state,
        };
        if record.state == ExecutionState::Stopped {
            record.finished_at = Some(Utc::now());
        }
        Ok(record.clone())
    }

    pub fn record(&self, execution_id: ExecutionId) -> Option<ExecutionRecord> {
        self.state.lock().ok()?.records.get(&execution_id).cloned()
    }

    pub fn records(&self) -> Vec<ExecutionRecord> {
        self.state
            .lock()
            .map(|state| state.records.values().cloned().collect())
            .unwrap_or_default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, BrokerState>, ExecutionError> {
        self.state
            .lock()
            .map_err(|_| ExecutionError::StateUnavailable)
    }
}

fn permit_from(record: &ExecutionRecord) -> ExecutionPermit {
    ExecutionPermit {
        execution_id: record.execution_id,
        target: record.target,
        command_hash: record.command_hash.clone(),
        risk: record.risk.clone(),
        mode: record.mode,
        command: record.command.clone(),
    }
}

fn ensure_active(state: &BrokerState, target: TargetRef) -> Result<(), ExecutionError> {
    match state.active_targets.get(&target.session_id) {
        Some(active) if *active == target => Ok(()),
        Some(_) => Err(ExecutionError::StaleTarget),
        None => Err(ExecutionError::TargetNotActive),
    }
}

fn invalidate_target(state: &mut BrokerState, target: TargetRef) {
    let now = Utc::now();
    for record in state.records.values_mut() {
        if record.target != target {
            continue;
        }
        let next = match record.state {
            ExecutionState::AwaitingApproval | ExecutionState::Authorized => {
                Some((ExecutionState::PausedDisconnected, "disconnected"))
            }
            ExecutionState::Running | ExecutionState::StopRequested => {
                Some((ExecutionState::UnknownAfterDisconnect, "disconnected"))
            }
            _ => None,
        };
        if let Some((next, code)) = next {
            record.state = next;
            record.error_code = Some(code.into());
            record.finished_at = Some(now);
        }
    }
}

pub fn assess_risk(command: &str) -> RiskAssessment {
    let trimmed = command.trim();
    if trimmed.is_empty() || command.contains('\0') {
        return risk(RiskLevel::Unknown, "empty_or_invalid_command");
    }
    if trimmed.contains("$(")
        || trimmed.contains('`')
        || trimmed.contains("\\x")
        || trimmed.contains("\\0")
    {
        return risk(RiskLevel::Unknown, "dynamic_or_encoded_shell_syntax");
    }
    let Ok(tokens) = lex_shell(trimmed) else {
        return risk(RiskLevel::Unknown, "shell_parse_failed");
    };
    if tokens.is_empty() {
        return risk(RiskLevel::Unknown, "empty_or_invalid_command");
    }
    let segments = command_segments(&tokens);
    if has_network_to_shell_pipeline(&segments) {
        return risk(RiskLevel::High, "network_content_piped_to_interpreter");
    }
    for segment in &segments {
        let Some(command_name) = segment.first().map(|token| command_basename(token)) else {
            return risk(RiskLevel::Unknown, "shell_parse_failed");
        };
        let high = matches!(
            command_name,
            "rm" | "rmdir"
                | "shred"
                | "wipefs"
                | "fdisk"
                | "parted"
                | "mount"
                | "umount"
                | "shutdown"
                | "reboot"
                | "halt"
                | "poweroff"
                | "kill"
                | "killall"
                | "pkill"
                | "useradd"
                | "userdel"
                | "usermod"
                | "passwd"
                | "iptables"
                | "nft"
                | "sudo"
                | "doas"
                | "eval"
        ) || command_name.starts_with("mkfs")
            || (command_name == "dd" && segment.iter().any(|token| token.starts_with("of=")))
            || (matches!(command_name, "chmod" | "chown" | "chgrp")
                && segment.iter().any(|token| {
                    token == "-R" || token.starts_with("--recursive") || token.starts_with('/')
                }))
            || (command_name == "systemctl"
                && segment.iter().any(|token| {
                    matches!(
                        token.as_str(),
                        "stop" | "disable" | "mask" | "restart" | "daemon-reload"
                    )
                }))
            || (command_name == "git"
                && ((segment.iter().any(|token| token == "reset")
                    && segment.iter().any(|token| token == "--hard"))
                    || (segment.iter().any(|token| token == "clean")
                        && segment.iter().any(|token| token.starts_with("-f")))))
            || segment
                .iter()
                .skip(1)
                .any(|token| is_sensitive_system_path(token));
        if high {
            return risk(
                RiskLevel::High,
                &format!("destructive_or_privileged_command:{command_name}"),
            );
        }
    }
    risk(RiskLevel::Low, "no_high_risk_rule_matched")
}

fn merge_model_risk(mut local: RiskAssessment, model: Option<RiskLevel>) -> RiskAssessment {
    if let Some(model) = model.filter(|model| *model > local.level) {
        local.level = model;
        local.reasons.push("model_risk_raised_local_level".into());
    }
    local
}

fn risk(level: RiskLevel, reason: &str) -> RiskAssessment {
    RiskAssessment {
        level,
        reasons: vec![reason.into()],
    }
}

fn command_basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

fn is_sensitive_system_path(token: &str) -> bool {
    ["/boot", "/dev", "/etc", "/proc", "/sys"]
        .iter()
        .any(|prefix| {
            token == *prefix
                || token
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
        || matches!(token, ">" | ">>" | "/" | "/*")
}

fn command_segments(tokens: &[String]) -> Vec<Vec<String>> {
    let mut result = vec![Vec::new()];
    for token in tokens {
        if matches!(
            token.as_str(),
            ";" | "|" | "&" | "&&" | "||" | ">" | ">>" | "<"
        ) {
            result.push(Vec::new());
        } else if let Some(segment) = result.last_mut() {
            segment.push(token.clone());
        }
    }
    result
}

fn has_network_to_shell_pipeline(segments: &[Vec<String>]) -> bool {
    let mut saw_network = false;
    for segment in segments {
        let Some(command) = segment.first().map(|token| command_basename(token)) else {
            continue;
        };
        if matches!(command, "curl" | "wget") {
            saw_network = true;
        } else if saw_network
            && matches!(
                command,
                "sh" | "bash" | "ash" | "zsh" | "python" | "python3" | "perl" | "ruby"
            )
        {
            return true;
        }
    }
    false
}

/// A deliberately small conservative lexer, not a shell evaluator. Separators
/// are preserved as tokens. Unsupported or malformed quoting never becomes Low.
fn lex_shell(command: &str) -> Result<Vec<String>, ()> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(character) = chars.next() {
        if let Some(active_quote) = quote {
            match character {
                character if character == active_quote => quote = None,
                '\\' if active_quote == '"' => {
                    word.push(chars.next().ok_or(())?);
                    started = true;
                }
                _ => {
                    word.push(character);
                    started = true;
                }
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                started = true;
            }
            '\\' => {
                word.push(chars.next().ok_or(())?);
                started = true;
            }
            character if character.is_whitespace() => {
                push_word(&mut tokens, &mut word, &mut started)
            }
            ';' | '|' | '&' | '>' | '<' => {
                push_word(&mut tokens, &mut word, &mut started);
                let mut separator = character.to_string();
                if matches!(character, '|' | '&' | '>') && chars.peek() == Some(&character) {
                    separator.push(chars.next().expect("peeked separator"));
                }
                tokens.push(separator);
            }
            _ => {
                word.push(character);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(());
    }
    push_word(&mut tokens, &mut word, &mut started);
    Ok(tokens)
}

fn push_word(tokens: &mut Vec<String>, word: &mut String, started: &mut bool) {
    if *started {
        tokens.push(std::mem::take(word));
        *started = false;
    }
}
