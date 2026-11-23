use crate::{AutomationError, BatchRun, BatchState, TargetRunState};
use sparrowshell_execution::{
    CommandExecutor, CommandProposal, CommandRequest, CommandSource, ExecutionBroker,
    ExecutionDecision, ExecutionState,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio::task::JoinSet;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct BatchEngine {
    runs: Arc<Mutex<HashMap<Uuid, BatchRun>>>,
    cancellations: Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
}

impl BatchEngine {
    pub fn create(&self, spec: crate::BatchSpec) -> Result<Uuid, AutomationError> {
        let run = BatchRun::new(spec)?;
        let id = run.id;
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        let mut cancellations = self
            .cancellations
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        runs.insert(id, run);
        cancellations.insert(id, Arc::new(AtomicBool::new(false)));
        Ok(id)
    }

    pub fn snapshot(&self, id: Uuid) -> Option<BatchRun> {
        self.runs.lock().ok()?.get(&id).cloned()
    }

    pub fn request_stop(&self, id: Uuid, broker: &ExecutionBroker) -> Result<(), AutomationError> {
        let cancel = self
            .cancellations
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?
            .get(&id)
            .cloned()
            .ok_or(AutomationError::InvalidWorkflow)?;
        cancel.store(true, Ordering::SeqCst);
        let run = self.snapshot(id).ok_or(AutomationError::StateUnavailable)?;
        for target in run.targets {
            if let Some(execution) = target.execution_id {
                let _ = broker.request_stop(execution);
            }
        }
        Ok(())
    }

    pub fn recover_interrupted(&self, id: Uuid) -> Result<BatchRun, AutomationError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        let run = runs.get_mut(&id).ok_or(AutomationError::StateUnavailable)?;
        run.mark_interrupted_after_restart();
        Ok(run.clone())
    }

    pub async fn run<E: CommandExecutor + 'static>(
        &self,
        id: Uuid,
        broker: ExecutionBroker,
        executor: Arc<E>,
    ) -> Result<BatchRun, AutomationError> {
        let (spec, cancel) = {
            let runs = self
                .runs
                .lock()
                .map_err(|_| AutomationError::StateUnavailable)?;
            let run = runs.get(&id).ok_or(AutomationError::StateUnavailable)?;
            if run.state != BatchState::Queued {
                return Err(AutomationError::StateUnavailable);
            }
            let cancel = self
                .cancellations
                .lock()
                .map_err(|_| AutomationError::StateUnavailable)?
                .get(&id)
                .cloned()
                .ok_or(AutomationError::StateUnavailable)?;
            (run.spec.clone(), cancel)
        };
        self.update(id, |run| run.state = BatchState::Running)?;
        let next = Arc::new(AtomicUsize::new(0));
        let mut workers = JoinSet::new();
        for _ in 0..spec.max_concurrent.min(spec.targets.len()) {
            let engine = self.clone();
            let broker = broker.clone();
            let executor = executor.clone();
            let spec = spec.clone();
            let next = next.clone();
            let cancel = cancel.clone();
            workers.spawn(async move {
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    if index >= spec.targets.len() {
                        break;
                    }
                    let target = spec.targets[index];
                    engine.update(id, |run| {
                        run.targets[index].state = TargetRunState::Running;
                    })?;
                    let request = CommandRequest {
                        source: CommandSource::Batch,
                        target,
                        proposal: CommandProposal {
                            command: spec.command.clone(),
                            model_risk: None,
                        },
                    };
                    let decision = broker.submit(request, spec.mode);
                    let permit = match decision {
                        Ok(ExecutionDecision::Execute(permit)) => permit,
                        Ok(ExecutionDecision::RequiresApproval(_)) | Err(_) => {
                            engine.update(id, |run| {
                                run.targets[index].state = TargetRunState::Failed;
                                run.targets[index].error_code = Some("execution_rejected".into());
                            })?;
                            continue;
                        }
                    };
                    let execution_id = permit.execution_id;
                    engine.update(id, |run| {
                        run.targets[index].execution_id = Some(execution_id);
                    })?;
                    let result = broker.execute(permit, executor.as_ref()).await;
                    engine.update(id, |run| {
                        let target = &mut run.targets[index];
                        match result {
                            Ok(record) => {
                                target.exit_status = record.exit_status;
                                target.error_code = record.error_code;
                                target.stdout = record.stdout;
                                target.stderr = record.stderr;
                                target.state = match record.state {
                                    ExecutionState::Succeeded => TargetRunState::Succeeded,
                                    ExecutionState::Stopped => TargetRunState::Stopped,
                                    ExecutionState::Unknown
                                    | ExecutionState::UnknownAfterDisconnect
                                    | ExecutionState::PausedDisconnected => TargetRunState::Unknown,
                                    _ => TargetRunState::Failed,
                                };
                            }
                            Err(_) => {
                                target.state = TargetRunState::Failed;
                                target.error_code = Some("execution_rejected".into());
                            }
                        }
                    })?;
                }
                Ok::<(), AutomationError>(())
            });
        }
        while let Some(result) = workers.join_next().await {
            result.map_err(|_| AutomationError::StateUnavailable)??;
        }
        self.update(id, |run| {
            if cancel.load(Ordering::SeqCst) {
                for target in &mut run.targets {
                    if target.state == TargetRunState::Queued {
                        target.state = TargetRunState::Stopped;
                        target.error_code = Some("cancelled_before_start".into());
                    }
                }
            }
            let succeeded = run
                .targets
                .iter()
                .filter(|target| target.state == TargetRunState::Succeeded)
                .count();
            run.state = if run
                .targets
                .iter()
                .any(|target| target.state == TargetRunState::Unknown)
            {
                BatchState::Unknown
            } else if succeeded == run.targets.len() {
                BatchState::Succeeded
            } else if run
                .targets
                .iter()
                .all(|target| target.state == TargetRunState::Stopped)
            {
                BatchState::Stopped
            } else if succeeded == 0 {
                BatchState::Failed
            } else {
                BatchState::PartiallySucceeded
            };
            run.finished_at = Some(chrono::Utc::now());
        })?;
        self.snapshot(id).ok_or(AutomationError::StateUnavailable)
    }

    fn update(&self, id: Uuid, change: impl FnOnce(&mut BatchRun)) -> Result<(), AutomationError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AutomationError::StateUnavailable)?;
        change(runs.get_mut(&id).ok_or(AutomationError::StateUnavailable)?);
        Ok(())
    }
}
