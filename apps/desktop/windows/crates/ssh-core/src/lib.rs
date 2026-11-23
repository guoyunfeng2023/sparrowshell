mod archive;
mod execution;
mod files;
mod monitor;
mod transfers;

pub use monitor::ProcessSignal;
use transfers::ActiveTransfer;

use russh::{
    client,
    keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyBase64},
    ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect,
};
use serde::{Deserialize, Serialize};
use sparrowshell_domain::{
    AuthType, ChannelId as LocalChannelId, ConnectionState, LocalServerRecord, ServerId,
    ServerInput, SessionId, SessionSnapshot, TargetRef, TransferId,
};
use sparrowshell_secrets::{CredentialRef, CredentialStore};
use sparrowshell_sftp_core::TransferEvent;
use sparrowshell_storage::{HostKeyRecord, Repository};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_SESSIONS: usize = 16;
const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyChallenge {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
    pub previous_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub session: Option<SessionSnapshot>,
    pub host_key_challenge: Option<HostKeyChallenge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub host_key_challenge: Option<HostKeyChallenge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEvent {
    pub target: TargetRef,
    pub channel_id: LocalChannelId,
    pub data: Vec<u8>,
}

pub trait EventSink: Send + Sync + 'static {
    fn terminal(&self, event: TerminalEvent);
    fn session(&self, event: SessionSnapshot);
    fn transfer(&self, _event: TransferEvent) {}
}

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("服务器配置无效：{0}")]
    InvalidServer(#[from] sparrowshell_domain::DomainError),
    #[error("服务器不存在")]
    ServerNotFound,
    #[error("凭据不存在，请先在本机保存凭据")]
    CredentialNotFound,
    #[error("无法读取本机钥匙串凭据，请检查钥匙串授权或重新保存密码")]
    CredentialUnavailable,
    #[error("SSH 认证失败")]
    AuthenticationFailed,
    #[error("私钥或私钥口令无法使用")]
    PrivateKeyFailed,
    #[error("连接失败或连接超时")]
    ConnectionFailed,
    #[error("主机指纹需要用户确认")]
    HostKeyRequired(Box<HostKeyChallenge>),
    #[error("没有可信任的主机指纹")]
    MissingHostKeyChallenge,
    #[error("指纹变更必须显式替换")]
    HostKeyReplacementRequired,
    #[error("会话不存在或已断开")]
    SessionNotFound,
    #[error("事件属于旧连接代次，已拒绝")]
    StaleGeneration,
    #[error("输入超过 64 KiB 限制")]
    InputTooLarge,
    #[error("会话或待确认指纹数量已达上限")]
    CapacityExceeded,
    #[error("本地状态不可用")]
    LocalStateUnavailable,
    #[error("远程操作失败，请检查权限和服务器状态")]
    RemoteOperationFailed,
    #[error("远程文件路径无效，已取消删除")]
    InvalidRemotePath,
    #[error("远程操作超时")]
    OperationTimeout,
    #[error("远程输出超过安全限制")]
    OutputTooLarge,
    #[error("进程标识无效")]
    InvalidProcessIdentity,
    #[error("进程已退出或启动标识已变化，未发送信号")]
    ProcessIdentityChanged,
    #[error("远程文件类型已变化，操作已取消")]
    RemoteFileTypeChanged,
    #[error("远程目标已存在，未覆盖")]
    RemoteFileAlreadyExists,
    #[error("解压目标已存在：{0}")]
    RemoteArchiveConflict(String),
    #[error("服务器缺少解压工具，请先安装 unzip 或 tar")]
    RemoteArchiveToolMissing,
    #[error("压缩包无法读取或已损坏")]
    RemoteArchiveInvalid,
    #[error("压缩包包含绝对路径或上级目录路径，已取消解压")]
    RemoteArchiveUnsafe,
    #[error("不支持这种压缩格式")]
    RemoteArchiveUnsupported,
    #[error("解压失败：{0}")]
    RemoteArchiveFailed(String),
    #[error("远程文本文件不能超过 1 MB")]
    RemoteTextFileTooLarge,
    #[error("远程文件包含二进制内容，不能作为文本编辑")]
    RemoteBinaryFile,
    #[error("远程文件不是有效的 UTF-8 文本")]
    RemoteTextInvalidUtf8,
    #[error("传输任务不存在或已经结束")]
    TransferNotFound,
}

struct HostKeyHandler {
    expected: Option<HostKeyRecord>,
    observed: Arc<Mutex<Option<HostKeyRecord>>>,
}

impl client::Handler for HostKeyHandler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = key.public_key();
        let record = HostKeyRecord {
            algorithm: key.algorithm().to_string(),
            public_key: key.public_key_base64(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        };
        let accepted = self.expected.as_ref() == Some(&record);
        if let Ok(mut observed) = self.observed.lock() {
            *observed = Some(record);
        }
        Ok(accepted)
    }
}

struct SessionRuntime {
    target: TargetRef,
    channel_id: LocalChannelId,
    writer: Arc<ChannelWriteHalf<client::Msg>>,
    transport: Arc<client::Handle<HostKeyHandler>>,
    _jump_transport: Option<client::Handle<HostKeyHandler>>,
}

#[derive(Default)]
struct ManagerState {
    sessions: HashMap<SessionId, SessionRuntime>,
    pending: HashMap<SessionId, HostKeyChallenge>,
    transfers: HashMap<TransferId, ActiveTransfer>,
    generations: HashMap<SessionId, u64>,
}

#[derive(Clone)]
pub struct SessionManager {
    repository: Arc<Mutex<Repository>>,
    credentials: Arc<dyn CredentialStore>,
    events: Arc<dyn EventSink>,
    state: Arc<Mutex<ManagerState>>,
}

impl SessionManager {
    pub fn new(
        repository: Arc<Mutex<Repository>>,
        credentials: Arc<dyn CredentialStore>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            repository,
            credentials,
            events,
            state: Arc::new(Mutex::new(ManagerState::default())),
        }
    }

    pub async fn connect(
        &self,
        server_id: ServerId,
        session_id: SessionId,
    ) -> Result<ConnectResult, SshError> {
        let generation = {
            let state = self
                .state
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?;
            if state.sessions.len() >= MAX_SESSIONS && !state.sessions.contains_key(&session_id) {
                return Err(SshError::CapacityExceeded);
            }
            if state.pending.len() >= MAX_SESSIONS && !state.pending.contains_key(&session_id) {
                return Err(SshError::CapacityExceeded);
            }
            let previous = state
                .sessions
                .get(&session_id)
                .map(|session| session.target.generation)
                .or_else(|| state.generations.get(&session_id).copied());
            match previous {
                Some(generation) => generation
                    .checked_add(1)
                    .ok_or(SshError::CapacityExceeded)?,
                None => 1,
            }
        };
        let server = self.server(server_id)?;
        let target = TargetRef {
            server_id,
            session_id,
            generation,
        };
        self.emit_state(target, ConnectionState::VerifyingTarget, None, None);
        match self.establish(&server, None).await {
            Ok((transport, jump_transport)) => {
                self.open_shell(target, transport, jump_transport).await
            }
            Err(SshError::HostKeyRequired(challenge)) => {
                self.state
                    .lock()
                    .map_err(|_| SshError::LocalStateUnavailable)?
                    .pending
                    .insert(session_id, (*challenge).clone());
                self.emit_state(target, ConnectionState::HostKeyBlocked, None, None);
                Ok(ConnectResult {
                    session: None,
                    host_key_challenge: Some(*challenge),
                })
            }
            Err(error) => {
                self.emit_state(
                    target,
                    ConnectionState::Error,
                    None,
                    Some("connection_failed".into()),
                );
                Err(error)
            }
        }
    }

    /// Tests an unsaved form without creating a session or persisting credentials.
    pub async fn test_connection(
        &self,
        input: ServerInput,
        secret: Option<zeroize::Zeroizing<String>>,
        accepted_host_key: Option<HostKeyChallenge>,
    ) -> Result<TestConnectionResult, SshError> {
        let input = input.validated()?;
        let credential_ref = input
            .id
            .and_then(|id| self.server(id).ok())
            .and_then(|server| server.credential_ref);
        let server = LocalServerRecord {
            id: input.id.unwrap_or_default(),
            folder_id: input.folder_id,
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            auth_type: input.auth_type,
            private_key_path_hint: input.private_key_path_hint,
            jump_server_id: input.jump_server_id,
            sort_order: 0,
            credential_ref,
            revision: 0,
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        };
        match self.establish(&server, accepted_host_key.as_ref()).await {
            Ok((mut transport, jump_transport)) => {
                self.authenticate_with_secret(&mut transport, &server, secret)
                    .await?;
                let _ = transport
                    .disconnect(Disconnect::ByApplication, "", "English")
                    .await;
                if let Some(jump_transport) = jump_transport {
                    let _ = jump_transport
                        .disconnect(Disconnect::ByApplication, "", "English")
                        .await;
                }
                Ok(TestConnectionResult {
                    host_key_challenge: None,
                })
            }
            Err(SshError::HostKeyRequired(challenge)) => Ok(TestConnectionResult {
                host_key_challenge: Some(*challenge),
            }),
            Err(error) => Err(error),
        }
    }

    async fn establish(
        &self,
        server: &LocalServerRecord,
        accepted_host_key: Option<&HostKeyChallenge>,
    ) -> Result<
        (
            client::Handle<HostKeyHandler>,
            Option<client::Handle<HostKeyHandler>>,
        ),
        SshError,
    > {
        if let Some(jump_id) = server.jump_server_id {
            let jump = self.server(jump_id)?;
            let stream = TcpStream::connect((jump.host.as_str(), jump.port));
            let stream = tokio::time::timeout(CONNECT_TIMEOUT, stream)
                .await
                .map_err(|_| SshError::ConnectionFailed)?
                .map_err(|_| SshError::ConnectionFailed)?;
            let mut jump_transport =
                handshake(stream, &jump, &self.repository, accepted_host_key).await?;
            self.authenticate(&mut jump_transport, &jump).await?;
            let channel = jump_transport
                .channel_open_direct_tcpip(server.host.clone(), server.port.into(), "127.0.0.1", 0)
                .await
                .map_err(|_| SshError::ConnectionFailed)?;
            let transport = handshake(
                channel.into_stream(),
                server,
                &self.repository,
                accepted_host_key,
            )
            .await?;
            Ok((transport, Some(jump_transport)))
        } else {
            let stream = TcpStream::connect((server.host.as_str(), server.port));
            let stream = tokio::time::timeout(CONNECT_TIMEOUT, stream)
                .await
                .map_err(|_| SshError::ConnectionFailed)?
                .map_err(|_| SshError::ConnectionFailed)?;
            let transport = handshake(stream, server, &self.repository, accepted_host_key).await?;
            Ok((transport, None))
        }
    }

    async fn authenticate(
        &self,
        transport: &mut client::Handle<HostKeyHandler>,
        server: &LocalServerRecord,
    ) -> Result<(), SshError> {
        self.authenticate_with_secret(transport, server, None).await
    }

    async fn authenticate_with_secret(
        &self,
        transport: &mut client::Handle<HostKeyHandler>,
        server: &LocalServerRecord,
        secret: Option<zeroize::Zeroizing<String>>,
    ) -> Result<(), SshError> {
        match server.auth_type {
            AuthType::Password => {
                let mut secret = match secret {
                    Some(secret) => secret,
                    None => {
                        let reference = server
                            .credential_ref
                            .as_deref()
                            .ok_or(SshError::CredentialNotFound)?;
                        let reference = CredentialRef::parse(reference.to_owned())
                            .map_err(|_| SshError::CredentialNotFound)?;
                        self.credentials
                            .load(&reference)
                            .map_err(credential_load_error)?
                    }
                };
                let result = transport
                    .authenticate_password(server.username.clone(), std::mem::take(&mut *secret))
                    .await
                    .map_err(|_| SshError::AuthenticationFailed)?;
                if result.success() {
                    Ok(())
                } else {
                    Err(SshError::AuthenticationFailed)
                }
            }
            AuthType::PrivateKey => {
                let hint = server
                    .private_key_path_hint
                    .as_deref()
                    .ok_or(SshError::PrivateKeyFailed)?;
                let path = expand_home(hint).ok_or(SshError::PrivateKeyFailed)?;
                let passphrase = match secret {
                    Some(secret) => Some(secret),
                    None => server
                        .credential_ref
                        .as_deref()
                        .and_then(|value| CredentialRef::parse(value.to_owned()).ok())
                        .and_then(|reference| self.credentials.load(&reference).ok()),
                };
                let key = russh::keys::load_secret_key(
                    path,
                    passphrase.as_ref().map(|secret| secret.as_str()),
                )
                .map_err(|_| SshError::PrivateKeyFailed)?;
                let hash = transport
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|_| SshError::AuthenticationFailed)?
                    .flatten();
                let result = transport
                    .authenticate_publickey(
                        server.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await
                    .map_err(|_| SshError::AuthenticationFailed)?;
                if result.success() {
                    Ok(())
                } else {
                    Err(SshError::AuthenticationFailed)
                }
            }
        }
    }

    async fn open_shell(
        &self,
        target: TargetRef,
        mut transport: client::Handle<HostKeyHandler>,
        jump_transport: Option<client::Handle<HostKeyHandler>>,
    ) -> Result<ConnectResult, SshError> {
        self.emit_state(target, ConnectionState::Authenticating, None, None);
        self.authenticate(&mut transport, &self.server(target.server_id)?)
            .await?;
        let channel = transport
            .channel_open_session()
            .await
            .map_err(|_| SshError::ConnectionFailed)?;
        channel
            .request_pty(true, "xterm-256color", 120, 32, 0, 0, &[])
            .await
            .map_err(|_| SshError::ConnectionFailed)?;
        channel
            .request_shell(true)
            .await
            .map_err(|_| SshError::ConnectionFailed)?;
        let (reader, writer) = channel.split();
        let channel_id = LocalChannelId::new();
        let snapshot = SessionSnapshot {
            target,
            state: ConnectionState::Connected,
            channel_id: Some(channel_id),
            error_code: None,
        };
        let old = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?;
            state.pending.remove(&target.session_id);
            Self::cancel_session_transfers(&mut state, target.session_id);
            state
                .generations
                .insert(target.session_id, target.generation);
            state.sessions.insert(
                target.session_id,
                SessionRuntime {
                    target,
                    channel_id,
                    writer: Arc::new(writer),
                    transport: Arc::new(transport),
                    _jump_transport: jump_transport,
                },
            )
        };
        if let Some(old) = old {
            let _ = old.writer.close().await;
        }
        self.events.session(snapshot.clone());
        spawn_output(reader, target, channel_id, self.events.clone());
        Ok(ConnectResult {
            session: Some(snapshot),
            host_key_challenge: None,
        })
    }

    pub async fn trust_host_key(
        &self,
        server_id: ServerId,
        session_id: SessionId,
        replace: bool,
    ) -> Result<ConnectResult, SshError> {
        let challenge = self
            .state
            .lock()
            .map_err(|_| SshError::LocalStateUnavailable)?
            .pending
            .get(&session_id)
            .cloned()
            .ok_or(SshError::MissingHostKeyChallenge)?;
        if challenge.previous_fingerprint.is_some() && !replace {
            return Err(SshError::HostKeyReplacementRequired);
        }
        let key = HostKeyRecord {
            algorithm: challenge.algorithm,
            public_key: challenge.public_key,
            fingerprint: challenge.fingerprint,
        };
        self.repository
            .lock()
            .map_err(|_| SshError::LocalStateUnavailable)?
            .trust_host_key(&challenge.host, challenge.port, &key)
            .map_err(|_| SshError::LocalStateUnavailable)?;
        self.connect(server_id, session_id).await
    }

    pub async fn terminal_write(
        &self,
        session_id: SessionId,
        generation: u64,
        data: Vec<u8>,
    ) -> Result<(), SshError> {
        if data.len() > MAX_INPUT_BYTES {
            return Err(SshError::InputTooLarge);
        }
        let writer = self.writer(session_id, generation)?;
        writer
            .data(data.as_slice())
            .await
            .map_err(|_| SshError::ConnectionFailed)
    }

    pub async fn terminal_resize(
        &self,
        session_id: SessionId,
        generation: u64,
        cols: u32,
        rows: u32,
    ) -> Result<(), SshError> {
        if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
            return Err(SshError::InputTooLarge);
        }
        self.writer(session_id, generation)?
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|_| SshError::ConnectionFailed)
    }

    pub async fn disconnect(&self, session_id: SessionId) -> Result<(), SshError> {
        let runtime = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?;
            let runtime = state
                .sessions
                .remove(&session_id)
                .ok_or(SshError::SessionNotFound)?;
            Self::cancel_session_transfers(&mut state, session_id);
            runtime
        };
        let _ = runtime.writer.close().await;
        let _ = runtime
            .transport
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        self.emit_state(
            runtime.target,
            ConnectionState::Disconnected,
            Some(runtime.channel_id),
            None,
        );
        Ok(())
    }

    /// Permanently closes a removed UI tab. A normal disconnect retains the
    /// generation so a manual reconnect can never reuse an older target.
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), SshError> {
        let runtime = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?;
            Self::cancel_session_transfers(&mut state, session_id);
            state.pending.remove(&session_id);
            state.generations.remove(&session_id);
            state.sessions.remove(&session_id)
        };
        if let Some(runtime) = runtime {
            let _ = runtime.writer.close().await;
            let _ = runtime
                .transport
                .disconnect(Disconnect::ByApplication, "", "English")
                .await;
            self.emit_state(
                runtime.target,
                ConnectionState::Disconnected,
                Some(runtime.channel_id),
                None,
            );
        }
        Ok(())
    }

    fn writer(
        &self,
        session_id: SessionId,
        generation: u64,
    ) -> Result<Arc<ChannelWriteHalf<client::Msg>>, SshError> {
        let state = self
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
        Ok(session.writer.clone())
    }

    fn transport(
        &self,
        session_id: SessionId,
        generation: u64,
    ) -> Result<(TargetRef, Arc<client::Handle<HostKeyHandler>>), SshError> {
        let state = self
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
        Ok((session.target, session.transport.clone()))
    }

    fn server(&self, id: ServerId) -> Result<LocalServerRecord, SshError> {
        self.repository
            .lock()
            .map_err(|_| SshError::LocalStateUnavailable)?
            .server(id)
            .map_err(|_| SshError::LocalStateUnavailable)?
            .ok_or(SshError::ServerNotFound)
    }

    fn emit_state(
        &self,
        target: TargetRef,
        state: ConnectionState,
        channel_id: Option<LocalChannelId>,
        error_code: Option<String>,
    ) {
        self.events.session(SessionSnapshot {
            target,
            state,
            channel_id,
            error_code,
        });
    }
}

async fn handshake<S>(
    stream: S,
    server: &LocalServerRecord,
    repository: &Arc<Mutex<Repository>>,
    accepted_host_key: Option<&HostKeyChallenge>,
) -> Result<client::Handle<HostKeyHandler>, SshError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let trusted = repository
        .lock()
        .map_err(|_| SshError::LocalStateUnavailable)?
        .trusted_host_key(&server.host, server.port)
        .map_err(|_| SshError::LocalStateUnavailable)?;
    let expected = if let Some(challenge) = accepted_host_key
        .filter(|challenge| challenge.host == server.host && challenge.port == server.port)
    {
        if trusted.as_ref().map(|key| key.fingerprint.as_str())
            != challenge.previous_fingerprint.as_deref()
        {
            return Err(SshError::MissingHostKeyChallenge);
        }
        Some(HostKeyRecord {
            algorithm: challenge.algorithm.clone(),
            public_key: challenge.public_key.clone(),
            fingerprint: challenge.fingerprint.clone(),
        })
    } else {
        trusted.clone()
    };
    let observed = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler {
        expected: expected.clone(),
        observed: observed.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        nodelay: true,
        ..Default::default()
    });
    let result = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect_stream(config, stream, handler),
    )
    .await;
    match result {
        Ok(Ok(handle)) => Ok(handle),
        _ => {
            let observed = observed
                .lock()
                .map_err(|_| SshError::LocalStateUnavailable)?
                .clone()
                .ok_or(SshError::ConnectionFailed)?;
            Err(SshError::HostKeyRequired(Box::new(HostKeyChallenge {
                host: server.host.clone(),
                port: server.port,
                algorithm: observed.algorithm,
                public_key: observed.public_key,
                fingerprint: observed.fingerprint,
                previous_fingerprint: trusted.map(|key| key.fingerprint),
            })))
        }
    }
}

fn spawn_output(
    mut reader: ChannelReadHalf,
    target: TargetRef,
    channel_id: LocalChannelId,
    events: Arc<dyn EventSink>,
) {
    tokio::spawn(async move {
        while let Some(message) = reader.wait().await {
            let data = match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => Some(data),
                ChannelMsg::Close | ChannelMsg::Eof => break,
                _ => None,
            };
            if let Some(data) = data {
                events.terminal(TerminalEvent {
                    target,
                    channel_id,
                    data: data.to_vec(),
                });
            }
        }
        events.session(SessionSnapshot {
            target,
            state: ConnectionState::Disconnected,
            channel_id: Some(channel_id),
            error_code: None,
        });
    });
}

fn expand_home(path: &str) -> Option<PathBuf> {
    if let Some(suffix) = path.strip_prefix("~/") {
        Some(std::env::var_os("HOME").map(PathBuf::from)?.join(suffix))
    } else if path.starts_with('/') {
        Some(PathBuf::from(path))
    } else {
        None
    }
}

fn credential_load_error(error: sparrowshell_secrets::SecretError) -> SshError {
    use sparrowshell_secrets::SecretError;

    match error {
        SecretError::NotFound | SecretError::InvalidReference => SshError::CredentialNotFound,
        SecretError::Unavailable | SecretError::OperationFailed => SshError::CredentialUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_absolute_or_explicit_home_private_key_paths_are_accepted() {
        assert_eq!(
            expand_home("/tmp/test-key"),
            Some(PathBuf::from("/tmp/test-key"))
        );
        assert_eq!(expand_home("../../secret"), None);
        assert_eq!(expand_home("~root/.ssh/id_ed25519"), None);
    }

    #[test]
    fn credential_store_permission_failures_are_not_reported_as_missing() {
        use sparrowshell_secrets::SecretError;

        for error in [SecretError::NotFound, SecretError::InvalidReference] {
            assert!(matches!(
                credential_load_error(error),
                SshError::CredentialNotFound
            ));
        }
        for error in [SecretError::Unavailable, SecretError::OperationFailed] {
            assert!(matches!(
                credential_load_error(error),
                SshError::CredentialUnavailable
            ));
        }
    }

    #[test]
    fn public_terminal_event_binds_all_target_dimensions() {
        let target = TargetRef {
            server_id: ServerId::new(),
            session_id: SessionId::new(),
            generation: 3,
        };
        let event = TerminalEvent {
            target,
            channel_id: LocalChannelId::new(),
            data: vec![1, 2, 3],
        };
        let json = serde_json::to_value(event).unwrap();
        assert_eq!(json["target"]["generation"], 3);
        assert!(json["channelId"].is_string());
    }
}
