use async_trait::async_trait;
use sparrowshell_ai_client::{AiError, AiOrchestrator, AiPlanState, AiStepState, StepProgress};
use sparrowshell_domain::{ServerId, SessionId, TargetRef};
use sparrowshell_execution::{
    CommandExecutor, CommandProposal, ExecutionBroker, ExecutionMode, ExecutionPermit,
    ExecutionState, ExecutorError, RemoteCommandResult,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

fn target() -> TargetRef {
    TargetRef {
        server_id: ServerId::new(),
        session_id: SessionId::new(),
        generation: 3,
    }
}

fn proposal(command: &str) -> CommandProposal {
    CommandProposal {
        command: command.into(),
        model_risk: None,
    }
}

struct FakeExecutor {
    calls: AtomicUsize,
    results: Mutex<Vec<Result<RemoteCommandResult, ExecutorError>>>,
}

impl FakeExecutor {
    fn successes() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            results: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl CommandExecutor for FakeExecutor {
    async fn execute(
        &self,
        _permit: &ExecutionPermit,
        _command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.results.lock().unwrap().pop().unwrap_or_else(|| {
            Ok(RemoteCommandResult {
                stdout: "real result\n".into(),
                stderr: String::new(),
                exit_status: Some(0),
            })
        })
    }
}

#[derive(Default)]
struct BlockingExecutor {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    result: Mutex<Option<Result<RemoteCommandResult, ExecutorError>>>,
}

#[async_trait]
impl CommandExecutor for BlockingExecutor {
    async fn execute(
        &self,
        _permit: &ExecutionPermit,
        _command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError> {
        self.started.notify_one();
        self.release.notified().await;
        self.result.lock().unwrap().take().unwrap()
    }
}

fn orchestrator(target: TargetRef) -> AiOrchestrator {
    let broker = ExecutionBroker::new();
    broker.activate_target(target).unwrap();
    AiOrchestrator::new(broker)
}

fn approval(progress: &StepProgress) -> sparrowshell_execution::PendingApproval {
    progress.approval.clone().expect("approval required")
}

#[tokio::test]
async fn every_step_re_reads_mode_and_returns_real_results() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(
            target,
            vec![proposal("printf first"), proposal("rm -rf /tmp/demo")],
        )
        .unwrap();
    let executor = FakeExecutor::successes();

    let first = orchestrator
        .run_next(
            plan.plan_id,
            target,
            ExecutionMode::AutoExceptHighRisk,
            &executor,
        )
        .await
        .unwrap();
    assert_eq!(first.execution.unwrap().state, ExecutionState::Succeeded);
    assert_eq!(first.plan.state, AiPlanState::Ready);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let second = orchestrator
        .run_next(
            plan.plan_id,
            target,
            ExecutionMode::AutoExceptHighRisk,
            &executor,
        )
        .await
        .unwrap();
    assert_eq!(second.plan.state, AiPlanState::AwaitingApproval);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let approval = approval(&second);

    let completed = orchestrator
        .approve_and_run(
            plan.plan_id,
            approval.approval_id,
            &approval.command_hash,
            target,
            &executor,
        )
        .await
        .unwrap();
    assert_eq!(completed.plan.state, AiPlanState::Completed);
    assert_eq!(completed.execution.unwrap().stdout, "real result\n");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn auto_all_runs_high_risk_without_hidden_confirmation() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("shutdown -h now")])
        .unwrap();
    let progress = orchestrator
        .run_next(
            plan.plan_id,
            target,
            ExecutionMode::AutoAll,
            &FakeExecutor::successes(),
        )
        .await
        .unwrap();
    assert!(progress.approval.is_none());
    assert_eq!(progress.plan.state, AiPlanState::Completed);
    assert_eq!(progress.execution.unwrap().state, ExecutionState::Succeeded);
}

#[tokio::test]
async fn stopping_a_queued_plan_prevents_all_future_commands() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("printf must-not-run")])
        .unwrap();
    let stopped = orchestrator.stop(plan.plan_id).unwrap();
    assert_eq!(stopped.state, AiPlanState::Stopped);
    assert_eq!(stopped.steps[0].state, AiStepState::Stopped);
    let executor = FakeExecutor::successes();
    assert!(matches!(
        orchestrator
            .run_next(plan.plan_id, target, ExecutionMode::AutoAll, &executor)
            .await,
        Err(AiError::PlanNotRunnable)
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stopping_running_command_waits_for_real_cancel_and_never_runs_next_step() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("sleep 30"), proposal("printf next")])
        .unwrap();
    let executor = Arc::new(BlockingExecutor {
        result: Mutex::new(Some(Err(ExecutorError::Cancelled))),
        ..BlockingExecutor::default()
    });
    let started = executor.started.clone();
    let release = executor.release.clone();
    let running = tokio::spawn({
        let orchestrator = orchestrator.clone();
        let executor = executor.clone();
        async move {
            orchestrator
                .run_next(
                    plan.plan_id,
                    target,
                    ExecutionMode::AutoAll,
                    executor.as_ref(),
                )
                .await
        }
    });
    started.notified().await;
    let stopping = orchestrator.stop(plan.plan_id).unwrap();
    assert_eq!(stopping.state, AiPlanState::Stopping);
    assert_eq!(stopping.steps[0].state, AiStepState::StopRequested);
    assert_eq!(stopping.steps[1].state, AiStepState::Stopped);
    release.notify_one();
    let result = running.await.unwrap().unwrap();
    assert_eq!(result.execution.unwrap().state, ExecutionState::Stopped);
    assert_eq!(result.plan.state, AiPlanState::Stopped);
    assert_eq!(result.plan.steps[1].state, AiStepState::Stopped);
}

#[tokio::test]
async fn disconnect_pauses_following_steps_and_reconnect_does_not_resume_unknown_work() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("sleep 30"), proposal("printf next")])
        .unwrap();
    let executor = Arc::new(BlockingExecutor {
        result: Mutex::new(Some(Err(ExecutorError::Disconnected))),
        ..BlockingExecutor::default()
    });
    let started = executor.started.clone();
    let release = executor.release.clone();
    let running = tokio::spawn({
        let orchestrator = orchestrator.clone();
        let executor = executor.clone();
        async move {
            orchestrator
                .run_next(
                    plan.plan_id,
                    target,
                    ExecutionMode::AutoAll,
                    executor.as_ref(),
                )
                .await
        }
    });
    started.notified().await;
    orchestrator.disconnected(target).unwrap();
    let paused = orchestrator.plan(plan.plan_id).unwrap();
    assert_eq!(paused.state, AiPlanState::PausedDisconnected);
    assert_eq!(paused.steps[0].state, AiStepState::UnknownAfterDisconnect);
    release.notify_one();
    let result = running.await.unwrap().unwrap();
    assert_eq!(
        result.execution.unwrap().state,
        ExecutionState::UnknownAfterDisconnect
    );
    assert_eq!(result.plan.state, AiPlanState::PausedDisconnected);

    let reconnected = TargetRef {
        generation: target.generation + 1,
        ..target
    };
    orchestrator.broker().activate_target(reconnected).unwrap();
    assert!(matches!(
        orchestrator
            .run_next(
                plan.plan_id,
                reconnected,
                ExecutionMode::AutoAll,
                &FakeExecutor::successes(),
            )
            .await,
        Err(AiError::PlanNotRunnable)
    ));
    assert!(matches!(
        orchestrator.resume(plan.plan_id, reconnected),
        Err(AiError::ManualRecoveryRequired)
    ));
}

#[tokio::test]
async fn explicit_resume_rebinds_queued_plan_and_old_approval_stays_invalid() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("rm -rf /tmp/demo")])
        .unwrap();
    let pending = orchestrator
        .run_next(
            plan.plan_id,
            target,
            ExecutionMode::AutoExceptHighRisk,
            &FakeExecutor::successes(),
        )
        .await
        .unwrap();
    let old_approval = approval(&pending);
    orchestrator.disconnected(target).unwrap();
    let reconnected = TargetRef {
        generation: target.generation + 1,
        ..target
    };
    orchestrator.broker().activate_target(reconnected).unwrap();
    let resumed = orchestrator.resume(plan.plan_id, reconnected).unwrap();
    assert_eq!(resumed.state, AiPlanState::Ready);
    assert_eq!(resumed.target, reconnected);

    assert!(orchestrator
        .approve_and_run(
            plan.plan_id,
            old_approval.approval_id,
            &old_approval.command_hash,
            reconnected,
            &FakeExecutor::successes(),
        )
        .await
        .is_err());
    let reevaluated = orchestrator
        .run_next(
            plan.plan_id,
            reconnected,
            ExecutionMode::AutoExceptHighRisk,
            &FakeExecutor::successes(),
        )
        .await
        .unwrap();
    assert!(reevaluated.approval.is_some());
    assert_ne!(
        reevaluated.approval.unwrap().approval_id,
        old_approval.approval_id
    );
}

#[tokio::test]
async fn plan_cannot_drift_to_another_server_or_session() {
    let target = target();
    let orchestrator = orchestrator(target);
    let plan = orchestrator
        .create_plan(target, vec![proposal("printf safe")])
        .unwrap();
    let other = TargetRef {
        server_id: ServerId::new(),
        session_id: SessionId::new(),
        generation: 3,
    };
    orchestrator.broker().activate_target(other).unwrap();
    assert!(matches!(
        orchestrator
            .run_next(
                plan.plan_id,
                other,
                ExecutionMode::AutoAll,
                &FakeExecutor::successes(),
            )
            .await,
        Err(AiError::TargetChanged)
    ));
}
