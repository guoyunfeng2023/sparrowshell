use super::{HostKeyHandler, SessionManager, SshError};
use russh::client;
use russh_sftp::client::SftpSession;
use sparrowshell_domain::SessionId;
use sparrowshell_sftp_core::{RemoteDirectory, RemoteFileKind};
use std::{sync::Arc, time::Duration};

const SFTP_TIMEOUT: Duration = Duration::from_secs(20);
const FAST_REMOVE_TIMEOUT: Duration = Duration::from_secs(600);

impl SessionManager {
    pub async fn list_remote_directory(
        &self,
        session_id: SessionId,
        generation: u64,
        path: Option<String>,
    ) -> Result<RemoteDirectory, SshError> {
        let (target, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::list(&session, target, path.as_deref()).await;
            let _ = session.close().await;
            result.map_err(|_| SshError::RemoteOperationFailed)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn create_remote_directory(
        &self,
        session_id: SessionId,
        generation: u64,
        parent: String,
        name: String,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::create_directory(&session, &parent, &name).await;
            let _ = session.close().await;
            result.map_err(|_| SshError::RemoteOperationFailed)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn create_remote_file(
        &self,
        session_id: SessionId,
        generation: u64,
        parent: String,
        name: String,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::create_file(&session, &parent, &name).await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn read_remote_text_file(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
    ) -> Result<String, SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::read_text_file(&session, &path).await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn write_remote_text_file(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        content: String,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::write_text_file(&session, &path, &content).await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn remove_remote_entry(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        expected_kind: RemoteFileKind,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::remove(&session, &path, expected_kind).await;
            let _ = session.close().await;
            result.map_err(|error| match error {
                sparrowshell_sftp_core::SftpError::FileTypeChanged => {
                    SshError::RemoteFileTypeChanged
                }
                _ => SshError::RemoteOperationFailed,
            })
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }
    pub async fn quick_remove_remote_entry(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        expected_kind: RemoteFileKind,
    ) -> Result<(), SshError> {
        let command = Self::quick_remove_command(&path)?;
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async {
            let session = open_sftp(transport.clone()).await?;
            let result =
                sparrowshell_sftp_core::verify_removal_target(&session, &path, expected_kind).await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)??;
        let result = self
            .exec_fixed_with_timeout(transport, &command, FAST_REMOVE_TIMEOUT)
            .await?;
        if result.exit_status == Some(0) {
            Ok(())
        } else {
            Err(SshError::RemoteOperationFailed)
        }
    }

    fn quick_remove_command(path: &str) -> Result<String, SshError> {
        if path.is_empty()
            || path.len() > 4096
            || !path.starts_with('/')
            || path.trim_matches('/').is_empty()
            || path.contains('\0')
            || path.split('/').any(|segment| matches!(segment, "." | ".."))
        {
            return Err(SshError::InvalidRemotePath);
        }
        let quoted = format!("'{}'", path.replace('\'', "'\"'\"'"));
        Ok(format!("rm -rf -- {quoted}"))
    }

    pub async fn rename_remote_entry(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        new_name: String,
        expected_kind: RemoteFileKind,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result =
                sparrowshell_sftp_core::rename(&session, &path, &new_name, expected_kind).await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }

    pub async fn move_remote_entry(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        destination_directory: String,
        expected_kind: RemoteFileKind,
    ) -> Result<(), SshError> {
        let (_, transport) = self.transport(session_id, generation)?;
        tokio::time::timeout(SFTP_TIMEOUT, async move {
            let session = open_sftp(transport).await?;
            let result = sparrowshell_sftp_core::move_to(
                &session,
                &path,
                &destination_directory,
                expected_kind,
            )
            .await;
            let _ = session.close().await;
            result.map_err(map_sftp_error)
        })
        .await
        .map_err(|_| SshError::OperationTimeout)?
    }
}

fn map_sftp_error(error: sparrowshell_sftp_core::SftpError) -> SshError {
    match error {
        sparrowshell_sftp_core::SftpError::FileTypeChanged => SshError::RemoteFileTypeChanged,
        sparrowshell_sftp_core::SftpError::AlreadyExists => SshError::RemoteFileAlreadyExists,
        sparrowshell_sftp_core::SftpError::TextFileTooLarge => SshError::RemoteTextFileTooLarge,
        sparrowshell_sftp_core::SftpError::BinaryFile => SshError::RemoteBinaryFile,
        sparrowshell_sftp_core::SftpError::InvalidUtf8 => SshError::RemoteTextInvalidUtf8,
        _ => SshError::RemoteOperationFailed,
    }
}

pub(crate) async fn open_sftp(
    transport: Arc<client::Handle<HostKeyHandler>>,
) -> Result<SftpSession, SshError> {
    let channel = transport
        .channel_open_session()
        .await
        .map_err(|_| SshError::ConnectionFailed)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|_| SshError::RemoteOperationFailed)?;
    let session = SftpSession::new(channel.into_stream())
        .await
        .map_err(|_| SshError::RemoteOperationFailed)?;
    session.set_timeout(15);
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::SessionManager;

    #[test]
    fn quick_remove_quotes_the_entire_remote_path_as_data() {
        assert_eq!(
            SessionManager::quick_remove_command("/tmp/report final.txt").unwrap(),
            "rm -rf -- '/tmp/report final.txt'"
        );
        assert_eq!(
            SessionManager::quick_remove_command("/tmp/it's $(literal)").unwrap(),
            "rm -rf -- '/tmp/it'\"'\"'s $(literal)'"
        );
    }

    #[test]
    fn quick_remove_rejects_root_relative_and_traversal_paths() {
        for path in [
            "",
            "/",
            "///",
            "relative",
            "/tmp/../etc",
            "/tmp/./etc",
            "/tmp/bad\0path",
        ] {
            assert!(
                SessionManager::quick_remove_command(path).is_err(),
                "accepted {path:?}"
            );
        }
    }
}
