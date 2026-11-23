//! Bounded, local-only batch and manual workflow orchestration.
//! All remote commands pass through ExecutionBroker; this crate has no SSH,
//! credential or shell-evaluator access.

mod batch;
mod model;
mod workflow;

pub use batch::*;
pub use model::*;
pub use workflow::*;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sparrowshell_domain::{ServerId, SessionId};
    use sparrowshell_execution::{
        CommandExecutor, CommandSource, ExecutionBroker, ExecutionMode, ExecutorError,
        RemoteCommandResult,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::sync::{watch, Notify};
    use uuid::Uuid;

    struct FakeExecutor {
        calls: AtomicUsize,
        started: Arc<Notify>,
        release: Arc<Notify>,
        blocking: bool,
    }

    #[async_trait]
    impl CommandExecutor for FakeExecutor {
        async fn execute(
            &self,
            permit: &sparrowshell_execution::ExecutionPermit,
            _command: &str,
        ) -> Result<RemoteCommandResult, ExecutorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_waiters();
            if self.blocking {
                self.release.notified().await;
                return Err(ExecutorError::Cancelled);
            }
            let number = permit.target.generation;
            Ok(RemoteCommandResult {
                stdout: if number == 2 {
                    "ready".into()
                } else {
                    String::new()
                },
                stderr: if number == 3 {
                    "failure detail".into()
                } else {
                    String::new()
                },
                exit_status: Some(if number == 3 { 1 } else { 0 }),
            })
        }
    }

    fn target(generation: u64) -> sparrowshell_domain::TargetRef {
        sparrowshell_domain::TargetRef {
            server_id: ServerId::new(),
            session_id: SessionId::new(),
            generation,
        }
    }

    fn executor() -> Arc<FakeExecutor> {
        Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            blocking: false,
        })
    }

    fn end(id: &str) -> WorkflowStep {
        WorkflowStep {
            id: id.into(),
            kind: WorkflowStepKind::End,
            on_success: None,
            on_failure: None,
        }
    }

    #[tokio::test]
    async fn batch_preserves_partial_success_and_target_identity() {
        let broker = ExecutionBroker::new();
        let targets = vec![target(1), target(2), target(3)];
        for target in &targets {
            broker.activate_target(*target).unwrap();
        }
        let engine = BatchEngine::default();
        let id = engine
            .create(BatchSpec {
                command: "printf test".into(),
                targets,
                max_concurrent: 2,
                mode: ExecutionMode::ConfirmEach,
            })
            .unwrap();
        let fake = executor();
        let run = engine.run(id, broker.clone(), fake.clone()).await.unwrap();
        assert_eq!(BatchState::PartiallySucceeded, run.state);
        assert_eq!(
            2,
            run.targets
                .iter()
                .filter(|target| target.state == TargetRunState::Succeeded)
                .count()
        );
        assert_eq!(Some(1), run.targets[2].exit_status);
        assert_eq!("ready", run.targets[1].stdout);
        assert_eq!("failure detail", run.targets[2].stderr);
        assert_eq!(3, fake.calls.load(Ordering::SeqCst));
        for target in run.targets {
            let execution = broker.record(target.execution_id.unwrap()).unwrap();
            assert_eq!(target.target, execution.target);
        }
    }

    #[tokio::test]
    async fn batch_can_stop_queued_work_and_restart_marks_unknown() {
        let broker = ExecutionBroker::new();
        let targets = vec![target(1), target(2), target(3)];
        for target in &targets {
            broker.activate_target(*target).unwrap();
        }
        let engine = BatchEngine::default();
        let id = engine
            .create(BatchSpec {
                command: "printf test".into(),
                targets,
                max_concurrent: 1,
                mode: ExecutionMode::AutoAll,
            })
            .unwrap();
        let mut fake = executor();
        Arc::get_mut(&mut fake).unwrap().blocking = true;
        let running = {
            let engine = engine.clone();
            let broker = broker.clone();
            let fake = fake.clone();
            tokio::spawn(async move { engine.run(id, broker, fake).await.unwrap() })
        };
        fake.started.notified().await;
        engine.request_stop(id, &broker).unwrap();
        fake.release.notify_waiters();
        let run = running.await.unwrap();
        assert_eq!(
            3,
            run.targets
                .iter()
                .filter(|target| target.state == TargetRunState::Stopped)
                .count()
        );
        assert_eq!(BatchState::Stopped, run.state);
        let mut recovered = BatchRun::new(BatchSpec {
            command: "printf test".into(),
            targets: vec![target(4)],
            max_concurrent: 1,
            mode: ExecutionMode::AutoAll,
        })
        .unwrap();
        recovered.mark_interrupted_after_restart();
        assert_eq!(BatchState::Unknown, recovered.state);
        assert_eq!(TargetRunState::Unknown, recovered.targets[0].state);
    }

    #[tokio::test]
    async fn workflow_versions_are_immutable_and_conditions_do_not_eval() {
        let catalog = WorkflowCatalog::default();
        let definition = catalog.create_definition("health check").unwrap();
        let steps = vec![
            WorkflowStep {
                id: "check".into(),
                kind: WorkflowStepKind::Command(CommandStep {
                    target: target(2),
                    command: "check-health".into(),
                    source: CommandSource::Workflow,
                    timeout_seconds: 10,
                    retries: 0,
                    retry_delay_seconds: 0,
                }),
                on_success: Some("condition".into()),
                on_failure: Some("failed".into()),
            },
            WorkflowStep {
                id: "condition".into(),
                kind: WorkflowStepKind::Condition(ConditionExpression::StdoutContains {
                    value: "ready".into(),
                }),
                on_success: Some("done".into()),
                on_failure: Some("failed".into()),
            },
            end("done"),
            end("failed"),
        ];
        let version =
            WorkflowVersion::publish(definition.id, 1, "check".into(), steps, 20).unwrap();
        catalog.publish(version.clone()).unwrap();
        assert!(matches!(
            catalog.publish(version.clone()),
            Err(AutomationError::VersionConflict)
        ));
        let broker = ExecutionBroker::new();
        let WorkflowStepKind::Command(command) = &version.steps[0].kind else {
            panic!("command step expected")
        };
        broker.activate_target(command.target).unwrap();
        let engine = WorkflowEngine::default();
        let id = engine
            .start(version.clone(), ExecutionMode::ConfirmEach)
            .unwrap();
        let (_tx, rx) = watch::channel(false);
        let result = engine
            .run(
                id,
                &broker,
                executor().as_ref(),
                ExecutionMode::ConfirmEach,
                rx,
            )
            .await
            .unwrap();
        assert_eq!(WorkflowRunState::Succeeded, result.run.state);
        assert_eq!("ready", result.run.steps[0].stdout);
        assert_eq!(version.content_hash, result.run.version_hash);
    }

    #[tokio::test]
    async fn workflow_ai_approval_resumes_the_same_step() {
        let broker = ExecutionBroker::new();
        let command_target = target(2);
        broker.activate_target(command_target).unwrap();
        let version = WorkflowVersion::publish(
            Uuid::new_v4(),
            1,
            "check".into(),
            vec![
                WorkflowStep {
                    id: "check".into(),
                    kind: WorkflowStepKind::Command(CommandStep {
                        target: command_target,
                        command: "check-health".into(),
                        source: CommandSource::Ai,
                        timeout_seconds: 10,
                        retries: 0,
                        retry_delay_seconds: 0,
                    }),
                    on_success: Some("done".into()),
                    on_failure: Some("failed".into()),
                },
                end("done"),
                WorkflowStep {
                    id: "failed".into(),
                    kind: WorkflowStepKind::Failure,
                    on_success: None,
                    on_failure: None,
                },
            ],
            20,
        )
        .unwrap();
        let engine = WorkflowEngine::default();
        let id = engine.start(version, ExecutionMode::ConfirmEach).unwrap();
        let (_tx, rx) = watch::channel(false);
        let pending = engine
            .run(
                id,
                &broker,
                executor().as_ref(),
                ExecutionMode::ConfirmEach,
                rx.clone(),
            )
            .await
            .unwrap();
        assert_eq!(WorkflowRunState::AwaitingApproval, pending.run.state);
        let approval = pending.pending_approval.unwrap();
        let completed = engine
            .approve_and_run(
                id,
                approval.approval_id,
                approval.command_hash,
                WorkflowRunContext {
                    broker: &broker,
                    executor: executor().as_ref(),
                    mode: ExecutionMode::ConfirmEach,
                    cancel: rx,
                },
            )
            .await
            .unwrap();
        assert_eq!(WorkflowRunState::Succeeded, completed.run.state);
        assert_eq!("ready", completed.run.steps.last().unwrap().stdout);
    }

    #[tokio::test]
    async fn workflow_failure_terminal_and_restart_recovery_are_truthful() {
        let broker = ExecutionBroker::new();
        let command_target = target(3);
        broker.activate_target(command_target).unwrap();
        let version = WorkflowVersion::publish(
            Uuid::new_v4(),
            1,
            "check".into(),
            vec![
                WorkflowStep {
                    id: "check".into(),
                    kind: WorkflowStepKind::Command(CommandStep {
                        target: command_target,
                        command: "false".into(),
                        source: CommandSource::Workflow,
                        timeout_seconds: 10,
                        retries: 0,
                        retry_delay_seconds: 0,
                    }),
                    on_success: Some("done".into()),
                    on_failure: Some("failed".into()),
                },
                end("done"),
                WorkflowStep {
                    id: "failed".into(),
                    kind: WorkflowStepKind::Failure,
                    on_success: None,
                    on_failure: None,
                },
            ],
            20,
        )
        .unwrap();
        let engine = WorkflowEngine::default();
        let id = engine
            .start(version.clone(), ExecutionMode::AutoAll)
            .unwrap();
        let (_tx, rx) = watch::channel(false);
        let failed = engine
            .run(id, &broker, executor().as_ref(), ExecutionMode::AutoAll, rx)
            .await
            .unwrap();
        assert_eq!(WorkflowRunState::Failed, failed.run.state);
        let interrupted = engine.start(version, ExecutionMode::AutoAll).unwrap();
        let recovered = engine.recover_interrupted(interrupted).unwrap();
        assert_eq!(WorkflowRunState::ResultUnknown, recovered.state);
        assert!(recovered.finished_at.is_some());
    }

    #[test]
    fn invalid_graph_and_unbounded_input_are_rejected() {
        assert!(WorkflowVersion::publish(
            Uuid::new_v4(),
            1,
            "missing".into(),
            vec![end("done")],
            20
        )
        .is_err());
        let mut looped = WorkflowStep {
            id: "wait".into(),
            kind: WorkflowStepKind::Wait(WaitStep { seconds: 1 }),
            on_success: Some("wait".into()),
            on_failure: None,
        };
        assert!(WorkflowVersion::publish(
            Uuid::new_v4(),
            1,
            "wait".into(),
            vec![looped.clone()],
            20
        )
        .is_err());
        looped.on_success = Some("done".into());
        assert!(WorkflowVersion::publish(
            Uuid::new_v4(),
            1,
            "wait".into(),
            vec![looped, end("done")],
            20
        )
        .is_ok());
        let duplicate = target(1);
        assert!(BatchRun::new(BatchSpec {
            command: "echo ok".into(),
            targets: vec![duplicate, duplicate],
            max_concurrent: 2,
            mode: ExecutionMode::AutoAll,
        })
        .is_err());
    }
}
