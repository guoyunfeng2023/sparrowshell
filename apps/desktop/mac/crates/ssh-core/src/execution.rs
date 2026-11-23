use super::SessionManager;
use async_trait::async_trait;
use russh::ChannelMsg;
use sparrowshell_execution::{
    CommandExecutor, ExecutionPermit, ExecutorError, RemoteCommandResult,
};
use std::time::Duration;

const AI_EXEC_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_AI_OUTPUT: usize = 4 * 1024 * 1024;

#[async_trait]
impl CommandExecutor for SessionManager {
    async fn execute(
        &self,
        permit: &ExecutionPermit,
        command: &str,
    ) -> Result<RemoteCommandResult, ExecutorError> {
        let (_, transport) = self
            .transport(permit.target.session_id, permit.target.generation)
            .map_err(|_| ExecutorError::Disconnected)?;
        tokio::time::timeout(AI_EXEC_TIMEOUT, async move {
            let mut channel = transport
                .channel_open_session()
                .await
                .map_err(|_| ExecutorError::Disconnected)?;
            channel
                .exec(true, command)
                .await
                .map_err(|_| ExecutorError::TransportFailed)?;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_status = None;
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        if stdout.len().saturating_add(data.len()) > MAX_AI_OUTPUT {
                            return Err(ExecutorError::TransportFailed);
                        }
                        stdout.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        if stderr.len().saturating_add(data.len()) > MAX_AI_OUTPUT {
                            return Err(ExecutorError::TransportFailed);
                        }
                        stderr.extend_from_slice(&data);
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
            Ok(RemoteCommandResult {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_status,
            })
        })
        .await
        // A timeout cannot prove that the remote process stopped.
        .map_err(|_| ExecutorError::Unknown)?
    }
}
