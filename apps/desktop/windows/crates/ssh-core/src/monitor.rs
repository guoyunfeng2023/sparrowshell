use super::{SessionManager, SshError};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use sparrowshell_domain::SessionId;
use sparrowshell_monitor::{parse_sample, SystemSnapshot, COLLECT_COMMAND};
use std::time::Duration;

const EXEC_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_EXEC_OUTPUT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSignal {
    Terminate,
    Kill,
}

pub(crate) struct ExecOutput {
    pub stdout: String,
    pub exit_status: Option<u32>,
}

impl SessionManager {
    pub async fn monitor_snapshot(
        &self,
        session_id: SessionId,
        generation: u64,
    ) -> Result<SystemSnapshot, SshError> {
        let (target, transport) = self.transport(session_id, generation)?;
        let result = self.exec_fixed(transport, COLLECT_COMMAND).await?;
        if result.exit_status.unwrap_or(0) != 0 {
            return Err(SshError::RemoteOperationFailed);
        }
        parse_sample(target, &result.stdout).map_err(|_| SshError::RemoteOperationFailed)
    }

    pub async fn terminate_process(
        &self,
        session_id: SessionId,
        generation: u64,
        pid: u32,
        start_id: String,
        signal: ProcessSignal,
    ) -> Result<(), SshError> {
        if pid == 0
            || start_id.is_empty()
            || start_id.len() > 64
            || !start_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, ' ' | ':')
            })
        {
            return Err(SshError::InvalidProcessIdentity);
        }
        let (_, transport) = self.transport(session_id, generation)?;
        let signal = match signal {
            ProcessSignal::Terminate => "TERM",
            ProcessSignal::Kill => "KILL",
        };
        let command = format!(
            "LC_ALL=C; export LC_ALL; actual=$(ps -p {pid} -o lstart=); actual=$(printf '%s' \"$actual\" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'); if [ \"$actual\" != \"{start_id}\" ]; then exit 65; fi; kill -{signal} {pid}"
        );
        let result = self.exec_fixed(transport, &command).await?;
        match result.exit_status.unwrap_or(0) {
            0 => Ok(()),
            65 => Err(SshError::ProcessIdentityChanged),
            _ => Err(SshError::RemoteOperationFailed),
        }
    }

    async fn exec_fixed(
        &self,
        transport: std::sync::Arc<russh::client::Handle<super::HostKeyHandler>>,
        command: &str,
    ) -> Result<ExecOutput, SshError> {
        self.exec_fixed_with_timeout(transport, command, EXEC_TIMEOUT)
            .await
    }

    pub(crate) async fn exec_fixed_with_timeout(
        &self,
        transport: std::sync::Arc<russh::client::Handle<super::HostKeyHandler>>,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecOutput, SshError> {
        tokio::time::timeout(timeout, async move {
            let mut channel = transport
                .channel_open_session()
                .await
                .map_err(|_| SshError::ConnectionFailed)?;
            channel
                .exec(true, command)
                .await
                .map_err(|_| SshError::RemoteOperationFailed)?;
            let mut bytes = Vec::new();
            let mut exit_status = None;
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        if bytes.len().saturating_add(data.len()) > MAX_EXEC_OUTPUT {
                            return Err(SshError::OutputTooLarge);
                        }
                        bytes.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        if bytes.len().saturating_add(data.len()) > MAX_EXEC_OUTPUT {
                            return Err(SshError::OutputTooLarge);
                        }
                        bytes.extend_from_slice(&data);
                    }
                    ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => {
                        exit_status = Some(status);
                    }
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok(ExecOutput {
                stdout: String::from_utf8(bytes).map_err(|_| SshError::RemoteOperationFailed)?,
                exit_status,
            })
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }
}
