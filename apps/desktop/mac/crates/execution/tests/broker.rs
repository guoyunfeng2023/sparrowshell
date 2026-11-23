use async_trait::async_trait;
use chrono::{Duration, Utc};
use sparrowshell_domain::{ServerId, SessionId, TargetRef};
use sparrowshell_execution::{
    assess_risk, CommandExecutor, CommandHash, CommandProposal, CommandRequest, CommandSource,
    ExecutionBroker, ExecutionDecision, ExecutionError, ExecutionMode, ExecutionState,
    ExecutorError, RemoteCommandResult, RiskLevel,
};
use std::sync::Mutex;

fn target() -> TargetRef {
    TargetRef {
        server_id: ServerId::new(),
        session_id: SessionId::new(),
        generation: 7,
    }
}

fn request(target: TargetRef, command: &str) -> CommandRequest {
    CommandRequest {
        source: CommandSource::Ai,
        target,
        proposal: CommandProposal {
            command: command.into(),
            model_risk: None,
        },
    }
}

fn active_broker(target: TargetRef) -> ExecutionBroker {
    let broker = ExecutionBroker::new();
    broker.activate_target(target).unwrap();
    broker
}

fn pending(decision: ExecutionDecision) -> sparrowshell_execution::PendingApproval {
    match decision {
        ExecutionDecision::RequiresApproval(approval) => approval,
        ExecutionDecision::Execute(_) => panic!("approval was required"),
    }
}

fn permit(decision: ExecutionDecision) -> sparrowshell_execution::ExecutionPermit {
    match decision {
        ExecutionDecision::Execute(permit) => permit,
        ExecutionDecision::RequiresApproval(_) => panic!("execution must not require approval"),
    }
}

struct FakeExecutor {
    result: Mutex<Option<Result<RemoteCommandResult, ExecutorError>>>,
}

impl FakeExecutor {
    fn success() -> Self {
        Self {
            result: Mutex::new(Some(Ok(RemoteCommandResult {
                stdout: "ok\n".into(),
                stderr: String::new(),
                exit_status: Some(0),
            }))),
        }
    }

    fn with(result: Result<RemoteCommandResult, ExecutorError>) -> Self {
        Self {
            result: Mutex::new(Some(result)),
        }
    }
}

#[async_trait]
impl CommandExecutor for FakeExecutor {
    async fn execute(
        &self,
        _permit: &sparrowshell_execution::ExecutionPermit,
        _command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError> {
        self.result.lock().unwrap().take().unwrap()
    }
}

#[derive(Default)]
struct BlockingExecutor {
    started: std::sync::Arc<tokio::sync::Notify>,
    release: std::sync::Arc<tokio::sync::Notify>,
}

#[async_trait]
impl CommandExecutor for BlockingExecutor {
    async fn execute(
        &self,
        _permit: &sparrowshell_execution::ExecutionPermit,
        _command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError> {
        self.started.notify_one();
        self.release.notified().await;
        Err(ExecutorError::Disconnected)
    }
}

#[test]
fn ai_three_mode_decision_matrix_is_exact() {
    for command in ["printf ok", "rm -rf /tmp/demo", "printf 'unterminated"] {
        let target = target();
        let broker = active_broker(target);
        let risk = assess_risk(command);
        let expected = match command {
            "printf ok" => RiskLevel::Low,
            "rm -rf /tmp/demo" => RiskLevel::High,
            _ => RiskLevel::Unknown,
        };
        assert_eq!(risk.level, expected);

        assert!(matches!(
            broker.submit(request(target, command), ExecutionMode::ConfirmEach),
            Ok(ExecutionDecision::RequiresApproval(_))
        ));
        let automatic = broker
            .submit(request(target, command), ExecutionMode::AutoExceptHighRisk)
            .unwrap();
        if expected == RiskLevel::Low {
            assert!(matches!(automatic, ExecutionDecision::Execute(_)));
        } else {
            assert!(matches!(automatic, ExecutionDecision::RequiresApproval(_)));
        }
        let auto_all = permit(
            broker
                .submit(request(target, command), ExecutionMode::AutoAll)
                .unwrap(),
        );
        assert_eq!(auto_all.risk.level, expected);
        assert_eq!(broker.records().len(), 3);
        assert!(broker
            .records()
            .iter()
            .all(|record| record.approval_id.is_none()
                || record.state == ExecutionState::AwaitingApproval));
    }
}

#[test]
fn batch_and_workflow_authorization_is_not_replaced_by_ai_mode() {
    let target = target();
    let broker = active_broker(target);
    for source in [CommandSource::Batch, CommandSource::Workflow] {
        let mut request = request(target, "rm -rf /tmp/authorized-batch-item");
        request.source = source;
        assert!(matches!(
            broker.submit(request, ExecutionMode::ConfirmEach),
            Ok(ExecutionDecision::Execute(_))
        ));
    }
}

#[test]
fn model_labels_can_raise_but_never_lower_local_risk() {
    let target = target();
    let broker = active_broker(target);
    let mut dangerous = request(target, "shutdown -h now");
    dangerous.proposal.model_risk = Some(RiskLevel::Low);
    assert!(matches!(
        broker.submit(dangerous, ExecutionMode::AutoExceptHighRisk),
        Ok(ExecutionDecision::RequiresApproval(_))
    ));
    let mut uncertain = request(target, "printf ok");
    uncertain.proposal.model_risk = Some(RiskLevel::High);
    assert!(matches!(
        broker.submit(uncertain, ExecutionMode::AutoExceptHighRisk),
        Ok(ExecutionDecision::RequiresApproval(_))
    ));
}

#[test]
fn approval_is_bound_to_exact_bytes_target_generation_and_expiry() {
    let target = target();
    let broker = active_broker(target);
    let approval = pending(
        broker
            .submit(
                request(target, "printf 'one byte'"),
                ExecutionMode::ConfirmEach,
            )
            .unwrap(),
    );
    let wrong_hash = CommandHash::from_command("printf 'one bytf'");
    assert!(matches!(
        broker.approve(approval.approval_id, &wrong_hash, target, Utc::now()),
        Err(ExecutionError::ApprovalMismatch)
    ));
    let other_target = TargetRef {
        generation: target.generation + 1,
        ..target
    };
    assert!(matches!(
        broker.approve(
            approval.approval_id,
            &approval.command_hash,
            other_target,
            Utc::now()
        ),
        Err(ExecutionError::ApprovalMismatch)
    ));
    assert!(matches!(
        broker.approve(
            approval.approval_id,
            &approval.command_hash,
            target,
            approval.expires_at + Duration::seconds(1),
        ),
        Err(ExecutionError::ApprovalExpired)
    ));
}

#[test]
fn approval_is_single_use_and_successful_approval_returns_original_command() {
    let target = target();
    let broker = active_broker(target);
    let approval = pending(
        broker
            .submit(
                request(target, "printf approved"),
                ExecutionMode::ConfirmEach,
            )
            .unwrap(),
    );
    let permit = broker
        .approve(
            approval.approval_id,
            &approval.command_hash,
            target,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(
        permit.command_hash,
        CommandHash::from_command("printf approved")
    );
    assert!(matches!(
        broker.approve(
            approval.approval_id,
            &approval.command_hash,
            target,
            Utc::now(),
        ),
        Err(ExecutionError::ApprovalNotFound)
    ));
}

#[test]
fn stale_target_and_unregistered_target_cannot_execute() {
    let old = target();
    let broker = active_broker(old);
    let new = TargetRef {
        generation: old.generation + 1,
        ..old
    };
    broker.activate_target(new).unwrap();
    assert!(matches!(
        broker.submit(request(old, "printf stale"), ExecutionMode::AutoAll),
        Err(ExecutionError::StaleTarget)
    ));
    assert!(matches!(
        broker.submit(request(target(), "printf unknown"), ExecutionMode::AutoAll),
        Err(ExecutionError::TargetNotActive)
    ));
}

#[tokio::test]
async fn audit_uses_real_exit_code_output_and_mode() {
    let target = target();
    let broker = active_broker(target);
    let success = permit(
        broker
            .submit(request(target, "printf ok"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let record = broker
        .execute(success, &FakeExecutor::success())
        .await
        .unwrap();
    assert_eq!(record.state, ExecutionState::Succeeded);
    assert_eq!(record.exit_status, Some(0));
    assert_eq!(record.stdout, "ok\n");
    assert_eq!(record.mode, ExecutionMode::AutoAll);
    assert_eq!(record.target, target);

    let failed = permit(
        broker
            .submit(request(target, "false"), ExecutionMode::AutoExceptHighRisk)
            .unwrap(),
    );
    let record = broker
        .execute(
            failed,
            &FakeExecutor::with(Ok(RemoteCommandResult {
                stdout: String::new(),
                stderr: "failed\n".into(),
                exit_status: Some(23),
            })),
        )
        .await
        .unwrap();
    assert_eq!(record.state, ExecutionState::Failed);
    assert_eq!(record.exit_status, Some(23));
    assert_eq!(record.stderr, "failed\n");
}

#[tokio::test]
async fn disconnect_makes_running_result_unknown_and_invalidates_queued_work() {
    let target = target();
    let broker = active_broker(target);
    let queued = permit(
        broker
            .submit(request(target, "printf queued"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let running = permit(
        broker
            .submit(request(target, "sleep 10"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let executor = BlockingExecutor::default();
    let started = executor.started.clone();
    let release = executor.release.clone();
    let execution = tokio::spawn({
        let broker = broker.clone();
        async move { broker.execute(running, &executor).await }
    });
    started.notified().await;
    broker.disconnect(target).unwrap();
    release.notify_one();
    let result = execution.await.unwrap().unwrap();
    assert_eq!(result.state, ExecutionState::UnknownAfterDisconnect);
    assert_eq!(
        broker.record(queued.execution_id).unwrap().state,
        ExecutionState::PausedDisconnected
    );
    assert!(matches!(
        broker.execute(queued, &FakeExecutor::success()).await,
        Err(ExecutionError::TargetNotActive)
    ));
}

#[tokio::test]
async fn missing_exit_status_and_cancellation_are_not_success() {
    let target = target();
    let broker = active_broker(target);
    let unknown = permit(
        broker
            .submit(request(target, "printf unknown"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let record = broker
        .execute(
            unknown,
            &FakeExecutor::with(Ok(RemoteCommandResult {
                stdout: "partial".into(),
                stderr: String::new(),
                exit_status: None,
            })),
        )
        .await
        .unwrap();
    assert_eq!(record.state, ExecutionState::Unknown);

    let stopped = permit(
        broker
            .submit(request(target, "sleep 10"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let record = broker
        .execute(stopped, &FakeExecutor::with(Err(ExecutorError::Cancelled)))
        .await
        .unwrap();
    assert_eq!(record.state, ExecutionState::Stopped);
}

#[tokio::test]
async fn stopping_unstarted_work_is_terminal_but_running_work_is_only_stop_requested() {
    let target = target();
    let broker = active_broker(target);
    let queued = permit(
        broker
            .submit(request(target, "printf queued"), ExecutionMode::AutoAll)
            .unwrap(),
    );
    let stopped = broker.request_stop(queued.execution_id).unwrap();
    assert_eq!(stopped.state, ExecutionState::Stopped);
    assert!(matches!(
        broker.execute(queued, &FakeExecutor::success()).await,
        Err(ExecutionError::InvalidExecutionState)
    ));
}
