use super::{files::open_sftp, ManagerState, SessionManager, SshError};
use sparrowshell_domain::{SessionId, TargetRef, TransferId};
use sparrowshell_sftp_core::{TransferDirection, TransferError, TransferEvent, TransferState};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MAX_ACTIVE_TRANSFERS: usize = 8;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct ActiveTransfer {
    pub target: TargetRef,
    pub cancelled: Arc<AtomicBool>,
}

impl SessionManager {
    pub async fn start_upload(
        &self,
        session_id: SessionId,
        generation: u64,
        local_path: String,
        remote_path: String,
        overwrite: bool,
    ) -> Result<TransferId, SshError> {
        self.start_transfer(
            session_id,
            generation,
            TransferDirection::Upload,
            local_path,
            remote_path,
            overwrite,
        )
    }

    pub async fn start_download(
        &self,
        session_id: SessionId,
        generation: u64,
        remote_path: String,
        local_path: String,
        overwrite: bool,
    ) -> Result<TransferId, SshError> {
        self.start_transfer(
            session_id,
            generation,
            TransferDirection::Download,
            remote_path,
            local_path,
            overwrite,
        )
    }

    fn start_transfer(
        &self,
        session_id: SessionId,
        generation: u64,
        direction: TransferDirection,
        source: String,
        destination: String,
        overwrite: bool,
    ) -> Result<TransferId, SshError> {
        let transfer_id = TransferId::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (target, transport) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?;
            let session = state
                .sessions
                .get(&session_id)
                .ok_or(SshError::SessionNotFound)?;
            if session.target.generation != generation {
                return Err(SshError::StaleGeneration);
            }
            if state.transfers.len() >= MAX_ACTIVE_TRANSFERS {
                return Err(SshError::CapacityExceeded);
            }
            let target = session.target;
            let transport = session.transport.clone();
            state.transfers.insert(
                transfer_id,
                ActiveTransfer {
                    target,
                    cancelled: cancelled.clone(),
                },
            );
            (target, transport)
        };
        let base = TransferEvent {
            transfer_id,
            target,
            direction,
            source,
            destination,
            bytes_total: 0,
            bytes_done: 0,
            bytes_per_second: 0,
            state: TransferState::Queued,
            error_code: None,
        };
        self.events.transfer(base.clone());
        let manager = self.clone();
        tokio::spawn(async move {
            manager
                .run_transfer(transport, base, overwrite, cancelled)
                .await;
        });
        Ok(transfer_id)
    }

    async fn run_transfer(
        &self,
        transport: Arc<russh::client::Handle<super::HostKeyHandler>>,
        base: TransferEvent,
        overwrite: bool,
        cancelled: Arc<AtomicBool>,
    ) {
        let started = Instant::now();
        let bytes_done = Arc::new(AtomicU64::new(0));
        let reported_bytes = bytes_done.clone();
        let events = self.events.clone();
        let progress_base = base.clone();
        let mut last_emit = Instant::now();
        let mut last_bytes = 0_u64;
        let progress = move |done: u64, total: u64| {
            reported_bytes.store(done, Ordering::Release);
            let now = Instant::now();
            if done > 0 && done < total && now.duration_since(last_emit) < PROGRESS_INTERVAL {
                return;
            }
            let elapsed = now.duration_since(last_emit).as_secs_f64();
            let speed = if elapsed > 0.0 {
                (done.saturating_sub(last_bytes) as f64 / elapsed).round() as u64
            } else {
                0
            };
            last_emit = now;
            last_bytes = done;
            events.transfer(TransferEvent {
                bytes_total: total,
                bytes_done: done,
                bytes_per_second: speed,
                state: TransferState::Running,
                error_code: None,
                ..progress_base.clone()
            });
        };
        let result = async {
            if cancelled.load(Ordering::Acquire) {
                return Err(TransferError::Cancelled);
            }
            let session = open_sftp(transport)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            let result = match base.direction {
                TransferDirection::Upload => {
                    sparrowshell_sftp_core::upload(
                        &session,
                        &base.source,
                        &base.destination,
                        overwrite,
                        cancelled.clone(),
                        progress,
                    )
                    .await
                }
                TransferDirection::Download => {
                    sparrowshell_sftp_core::download(
                        &session,
                        &base.source,
                        &base.destination,
                        overwrite,
                        cancelled.clone(),
                        progress,
                    )
                    .await
                }
            };
            let _ = session.close().await;
            result
        }
        .await;
        let (state, error_code, total) = match result {
            Ok(total) => (TransferState::Completed, None, total),
            Err(error) => {
                let state = if error == TransferError::Cancelled {
                    TransferState::Cancelled
                } else {
                    TransferState::Failed
                };
                (state, Some(transfer_error_code(&error).to_owned()), 0)
            }
        };
        let elapsed = started.elapsed().as_secs_f64();
        let done = bytes_done.load(Ordering::Acquire);
        if let Ok(mut state) = self.state.lock() {
            state.transfers.remove(&base.transfer_id);
        }
        self.events.transfer(TransferEvent {
            bytes_total: total,
            bytes_done: done,
            bytes_per_second: if elapsed > 0.0 {
                (done as f64 / elapsed).round() as u64
            } else {
                0
            },
            state,
            error_code,
            ..base
        });
    }

    pub fn cancel_transfer(
        &self,
        transfer_id: TransferId,
        session_id: SessionId,
        generation: u64,
    ) -> Result<(), SshError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SshError::LocalStateUnavailable)?;
        let transfer = state
            .transfers
            .get(&transfer_id)
            .ok_or(SshError::TransferNotFound)?;
        if transfer.target.session_id != session_id {
            return Err(SshError::SessionNotFound);
        }
        if transfer.target.generation != generation {
            return Err(SshError::StaleGeneration);
        }
        transfer.cancelled.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn cancel_session_transfers(state: &mut ManagerState, session_id: SessionId) {
        state.transfers.retain(|_, transfer| {
            if transfer.target.session_id == session_id {
                transfer.cancelled.store(true, Ordering::Release);
                false
            } else {
                true
            }
        });
    }
}

fn transfer_error_code(error: &TransferError) -> &'static str {
    match error {
        TransferError::AlreadyExists => "already_exists",
        TransferError::Cancelled => "cancelled",
        TransferError::InvalidPath => "invalid_path",
        TransferError::OperationFailed => "operation_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_stable_frontend_codes() {
        assert_eq!(
            transfer_error_code(&TransferError::AlreadyExists),
            "already_exists"
        );
        assert_eq!(transfer_error_code(&TransferError::Cancelled), "cancelled");
        assert_eq!(
            transfer_error_code(&TransferError::InvalidPath),
            "invalid_path"
        );
        assert_eq!(
            transfer_error_code(&TransferError::OperationFailed),
            "operation_failed"
        );
    }

    #[test]
    fn cancelling_a_session_does_not_cancel_other_sessions() {
        let session_id = SessionId::new();
        let other_session = SessionId::new();
        let kept_id = TransferId::new();
        let cancelled_id = TransferId::new();
        let kept = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut state = ManagerState::default();
        for (id, target_session, flag) in [
            (kept_id, other_session, kept.clone()),
            (cancelled_id, session_id, cancelled.clone()),
        ] {
            state.transfers.insert(
                id,
                ActiveTransfer {
                    target: TargetRef {
                        server_id: sparrowshell_domain::ServerId::new(),
                        session_id: target_session,
                        generation: 1,
                    },
                    cancelled: flag,
                },
            );
        }
        SessionManager::cancel_session_transfers(&mut state, session_id);
        assert!(!kept.load(Ordering::Acquire));
        assert!(cancelled.load(Ordering::Acquire));
        assert!(state.transfers.contains_key(&kept_id));
        assert!(!state.transfers.contains_key(&cancelled_id));
    }
}
