mod local_account_store;
mod native_appearance;
mod native_menu;

use local_account_store::LocalAccountStore;
use sparrowshell_ai_client::{
    billing::{AiEntitlement, BillingClient, BillingOrder},
    context::{ContextBuilder, ContextCategory, ContextInput},
    gateway::{AgentTurnResult, AiGateway, ConversationMessage},
    AiOrchestrator, AiPlan, PlanId, StepProgress,
};
use sparrowshell_automation::{
    BatchEngine, BatchRun, BatchSpec, BatchState, WorkflowEngine, WorkflowExecution, WorkflowRun,
    WorkflowRunContext, WorkflowRunState, WorkflowVersion,
};
use sparrowshell_domain::SessionSnapshot;
use sparrowshell_domain::{
    AutomationDefinitionSyncDto, CatalogOrderItem, CatalogSnapshot, CommandHistoryEntry, Folder,
    FolderId, LocalServerRecord, ServerId, ServerInput, SessionId, SyncEntityType, TargetRef,
    TransferId,
};
use sparrowshell_execution::{
    ApprovalId, CommandHash, CommandProposal, ExecutionId, ExecutionMode, ExecutionState,
};
use sparrowshell_monitor::SystemSnapshot;
use sparrowshell_secrets::{CredentialRef, CredentialStore, PlatformCredentialStore};
use sparrowshell_sftp_core::{RemoteDirectory, RemoteFileKind, TransferEvent};
use sparrowshell_ssh_core::{
    ConnectResult, EventSink, HostKeyChallenge, ProcessSignal, SessionManager, TerminalEvent,
    TestConnectionResult,
};
use sparrowshell_storage::{
    AccountSwitchChoice, AutomationRunRecord, Repository, SyncConflictRecord,
};
use sparrowshell_sync::{
    download_cloud_snapshot, AccountSession, AccountStatus, AccountTokenStore, CloudClient,
    FeedbackReceipt, HttpSyncTransport, SyncEngine, SyncReport, UpdateCheck,
};
use std::{
    fs,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};
use zeroize::Zeroizing;

struct TauriEvents(tauri::AppHandle);
impl EventSink for TauriEvents {
    fn terminal(&self, event: TerminalEvent) {
        let _ = self.0.emit("terminal-output", event);
    }
    fn session(&self, event: SessionSnapshot) {
        let _ = self.0.emit("session-state", event);
    }
    fn transfer(&self, event: TransferEvent) {
        let _ = self.0.emit("transfer-progress", event);
    }
}

struct AppState {
    repository: Arc<Mutex<Repository>>,
    credentials: PlatformCredentialStore,
    accounts: AccountTokenStore<LocalAccountStore>,
    sessions: SessionManager,
    ai: AiOrchestrator,
    gateway: AiGateway,
    batch: BatchEngine,
    workflows: WorkflowEngine,
    workflow_cancellations:
        Mutex<std::collections::HashMap<uuid::Uuid, tokio::sync::watch::Sender<bool>>>,
    sync_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct AutoSyncState {
    failures: u32,
    next_attempt: Option<std::time::Instant>,
}

impl AppState {
    fn repository(&self) -> Result<std::sync::MutexGuard<'_, Repository>, String> {
        self.repository
            .lock()
            .map_err(|_| "本地存储暂时不可用".to_owned())
    }
}

#[tauri::command]
fn read_clipboard() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板".to_owned())?;
    clipboard
        .get_text()
        .map_err(|_| "剪贴板中没有可粘贴的文本".to_owned())
}

#[tauri::command]
fn catalog_snapshot(state: State<'_, AppState>) -> Result<CatalogSnapshot, String> {
    state
        .repository()?
        .snapshot()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_folder(
    parent_id: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> Result<Folder, String> {
    let parent_id = parent_id
        .map(|id| FolderId::from_str(&id))
        .transpose()
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .create_folder(parent_id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_folder(
    id: String,
    parent_id: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> Result<Folder, String> {
    let id = FolderId::from_str(&id).map_err(|error| error.to_string())?;
    let parent_id = parent_id
        .map(|id| FolderId::from_str(&id))
        .transpose()
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_folder(id, parent_id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reorder_folders(items: Vec<CatalogOrderItem>, state: State<'_, AppState>) -> Result<(), String> {
    state
        .repository()?
        .reorder_folders(items)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reorder_servers(items: Vec<CatalogOrderItem>, state: State<'_, AppState>) -> Result<(), String> {
    state
        .repository()?
        .reorder_servers(items)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_server(
    input: ServerInput,
    state: State<'_, AppState>,
) -> Result<LocalServerRecord, String> {
    state
        .repository()?
        .upsert_server(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_password(
    server_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    let repository = state.repository()?;
    let server = repository
        .server(server_id)
        .map_err(|error| error.to_string())?
        .ok_or("服务器不存在")?;
    let reference = match server.credential_ref {
        Some(value) => CredentialRef::parse(value).map_err(|error| error.to_string())?,
        None => CredentialRef::new_ssh(),
    };
    state
        .credentials
        .save(&reference, Zeroizing::new(password))
        .map_err(|error| error.to_string())?;
    repository
        .set_credential_ref(server_id, Some(reference.as_str().to_owned()))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_server(server_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    let reference = state
        .repository()?
        .delete_server(server_id)
        .map_err(|error| error.to_string())?;
    if let Some(reference) = reference {
        let reference = CredentialRef::parse(reference).map_err(|error| error.to_string())?;
        state
            .credentials
            .delete(&reference)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn duplicate_server(
    server_id: String,
    state: State<'_, AppState>,
) -> Result<LocalServerRecord, String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    state
        .repository()?
        .duplicate_server(server_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_folder(folder_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let folder_id = FolderId::from_str(&folder_id).map_err(|error| error.to_string())?;
    let references = state
        .repository()?
        .delete_folder(folder_id)
        .map_err(|error| error.to_string())?;
    for reference in references {
        let reference = CredentialRef::parse(reference).map_err(|error| error.to_string())?;
        state
            .credentials
            .delete(&reference)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn connect(
    server_id: String,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<ConnectResult, String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    let result = state
        .sessions
        .connect(server_id, session_id)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(session) = &result.session {
        state
            .ai
            .broker()
            .activate_target(session.target)
            .map_err(|error| error.to_string())?;
    }
    Ok(result)
}

#[tauri::command]
async fn test_server_connection(
    input: ServerInput,
    secret: Option<String>,
    accepted_host_key: Option<HostKeyChallenge>,
    state: State<'_, AppState>,
) -> Result<TestConnectionResult, String> {
    state
        .sessions
        .test_connection(input, secret.map(Zeroizing::new), accepted_host_key)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn trust_host_key(
    server_id: String,
    session_id: String,
    replace: bool,
    state: State<'_, AppState>,
) -> Result<ConnectResult, String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    let result = state
        .sessions
        .trust_host_key(server_id, session_id, replace)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(session) = &result.session {
        state
            .ai
            .broker()
            .activate_target(session.target)
            .map_err(|error| error.to_string())?;
    }
    Ok(result)
}

#[tauri::command]
async fn terminal_write(
    session_id: String,
    generation: u64,
    data: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .terminal_write(session_id, generation, data)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn terminal_resize(
    session_id: String,
    generation: u64,
    cols: u32,
    rows: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .terminal_resize(session_id, generation, cols, rows)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn disconnect(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .disconnect(session_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn close_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .close_session(session_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn record_command(
    server_id: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<Option<CommandHistoryEntry>, String> {
    let server_id = ServerId::from_str(&server_id).map_err(|error| error.to_string())?;
    state
        .repository()?
        .record_command(server_id, &command)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn command_history(
    server_id: Option<String>,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<CommandHistoryEntry>, String> {
    let server_id = server_id
        .map(|id| ServerId::from_str(&id))
        .transpose()
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .command_history(server_id, limit.unwrap_or(100))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_command_history(
    server_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let server_id = server_id
        .map(|id| ServerId::from_str(&id))
        .transpose()
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .clear_command_history(server_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn account_status(state: State<'_, AppState>) -> Result<AccountStatus, String> {
    state.accounts.status().map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountAuthResult {
    session: AccountSession,
    requires_choice: bool,
    previous_user_id: Option<String>,
    local_folders: usize,
    local_servers: usize,
}

fn prepare_account_auth(
    state: &State<'_, AppState>,
    session: AccountSession,
) -> Result<AccountAuthResult, String> {
    let repository = state.repository()?;
    let previous_user_id = repository
        .workspace_owner()
        .map_err(|error| error.to_string())?;
    let snapshot = repository
        .workspace_snapshot()
        .map_err(|error| error.to_string())?;
    let has_local_data = !snapshot.catalog.folders.is_empty()
        || !snapshot.catalog.servers.is_empty()
        || !snapshot.automations.is_empty();
    let requires_choice =
        previous_user_id.as_deref() != Some(session.user_id.as_str()) && has_local_data;
    if !requires_choice && previous_user_id.as_deref() != Some(session.user_id.as_str()) {
        repository
            .set_workspace_owner(&session.user_id)
            .map_err(|error| error.to_string())?;
    }
    Ok(AccountAuthResult {
        session,
        requires_choice,
        previous_user_id,
        local_folders: snapshot.catalog.folders.len(),
        local_servers: snapshot.catalog.servers.len(),
    })
}

#[tauri::command]
async fn account_login(
    base_url: String,
    email: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<AccountAuthResult, String> {
    let password = Zeroizing::new(password);
    let email = email.trim().to_owned();
    if email.len() > 320 || !email.contains('@') || password.is_empty() || password.len() > 1024 {
        return Err("登录参数无效".to_owned());
    }
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let device_id = state
        .repository()?
        .sync_device_id()
        .map_err(|error| error.to_string())?;
    let tokens = client
        .login(&email, password.to_string(), device_id)
        .await
        .map_err(|error| error.to_string())?;
    let _session = state.sync_lock.lock().await;
    let session = state
        .accounts
        .save(tokens)
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .set_cloud_base_url(&base_url)
        .map_err(|error| error.to_string())?;
    prepare_account_auth(&state, session)
}

fn valid_account_password(password: &str) -> bool {
    password.len() >= 8
        && password.len() <= 256
        && password.bytes().all(|value| (33..=126).contains(&value))
        && password.bytes().any(|value| value.is_ascii_alphabetic())
        && password.bytes().any(|value| !value.is_ascii_alphabetic())
}

fn valid_verification_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|value| value.is_ascii_digit())
}

#[tauri::command]
async fn account_send_verification_code(
    base_url: String,
    email: String,
    purpose: String,
    locale: String,
) -> Result<(), String> {
    let email = email.trim().to_owned();
    if email.len() > 320
        || !email.contains('@')
        || !matches!(purpose.as_str(), "register" | "password_reset")
    {
        return Err("邮箱或验证码用途无效".to_owned());
    }
    CloudClient::new(&base_url)
        .map_err(|error| error.to_string())?
        .send_verification_code(&email, &purpose, &locale)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn account_register(
    base_url: String,
    email: String,
    password: String,
    code: String,
    state: State<'_, AppState>,
) -> Result<AccountAuthResult, String> {
    let password = Zeroizing::new(password);
    let email = email.trim().to_owned();
    if email.len() > 320
        || !email.contains('@')
        || !valid_account_password(&password)
        || !valid_verification_code(code.trim())
    {
        return Err(
            "注册参数无效：密码至少 8 位且包含字母及数字或符号，验证码为 6 位数字".to_owned(),
        );
    }
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let device_id = state
        .repository()?
        .sync_device_id()
        .map_err(|error| error.to_string())?;
    let tokens = client
        .register(&email, password.to_string(), code.trim(), device_id)
        .await
        .map_err(|error| error.to_string())?;
    let _session = state.sync_lock.lock().await;
    let session = state
        .accounts
        .save(tokens)
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .set_cloud_base_url(&base_url)
        .map_err(|error| error.to_string())?;
    prepare_account_auth(&state, session)
}

#[tauri::command]
async fn account_reset_password(
    base_url: String,
    email: String,
    code: String,
    new_password: String,
) -> Result<(), String> {
    let email = email.trim().to_owned();
    let new_password = Zeroizing::new(new_password);
    if email.len() > 320
        || !email.contains('@')
        || !valid_verification_code(code.trim())
        || !valid_account_password(&new_password)
    {
        return Err("重置参数无效：请检查邮箱、验证码和新密码".to_owned());
    }
    CloudClient::new(&base_url)
        .map_err(|error| error.to_string())?
        .reset_password(&email, code.trim(), new_password.to_string())
        .await
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountLogoutResult {
    remote_revoked: bool,
}

#[tauri::command]
async fn account_logout(state: State<'_, AppState>) -> Result<AccountLogoutResult, String> {
    let _session = state.sync_lock.lock().await;
    let (base_url, tokens) = {
        let repository = state.repository()?;
        (
            repository
                .cloud_base_url()
                .map_err(|error| error.to_string())?,
            state.accounts.tokens().map_err(|error| error.to_string())?,
        )
    };
    let mut remote_revoked = false;
    if let (Some(base_url), Some(tokens)) = (base_url, tokens) {
        if let Ok(client) = CloudClient::new(&base_url) {
            remote_revoked = client.logout(&tokens.access_token).await.is_ok();
        }
    }
    state
        .accounts
        .sign_out()
        .map_err(|error| error.to_string())?;
    Ok(AccountLogoutResult { remote_revoked })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSwitchResult {
    applied: bool,
    sync_report: Option<SyncReport>,
    sync_error: Option<String>,
    outbox: usize,
    conflicts: usize,
}

#[tauri::command]
async fn account_switch(
    choice: AccountSwitchChoice,
    state: State<'_, AppState>,
) -> Result<AccountSwitchResult, String> {
    let _session = state.sync_lock.lock().await;
    let tokens = state
        .accounts
        .tokens()
        .map_err(|error| error.to_string())?
        .ok_or("当前未登录")?;
    let account_id = tokens.user_id.clone();
    {
        let repository = state.repository()?;
        if repository
            .workspace_owner()
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some(account_id.as_str())
        {
            return Err("当前账号无需切换".to_owned());
        }
    }
    let base_url = platform_base_url(&state)?;
    let account_store = {
        let repository = state.repository()?;
        LocalAccountStore::new(
            repository
                .data_directory()
                .ok_or("本地账号存储目录不可用")?,
        )
    };
    let transport =
        HttpSyncTransport::new(&base_url, account_store).map_err(|error| error.to_string())?;
    let (cloud_changes, cloud_cursor) = download_cloud_snapshot(&transport)
        .await
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .apply_account_switch(&account_id, choice, &cloud_changes, cloud_cursor)
        .map_err(|error| error.to_string())?;

    let repository = state.repository.clone();
    let sync_account_id = account_id.clone();
    let sync_result = tauri::async_runtime::spawn_blocking(move || {
        let repository = repository
            .lock()
            .map_err(|_| "本地存储暂时不可用".to_owned())?;
        run_sync(&repository, &base_url, &sync_account_id)
    })
    .await
    .map_err(|_| "账号切换后的同步任务已中断".to_owned())
    .and_then(|result| result);
    let (sync_report, sync_error) = match sync_result {
        Ok(report) => (Some(report), None),
        Err(error) => (None, Some(error)),
    };
    let repository = state.repository()?;
    Ok(AccountSwitchResult {
        applied: true,
        sync_report,
        sync_error,
        outbox: repository.outbox_len().map_err(|error| error.to_string())?,
        conflicts: repository
            .unresolved_conflicts()
            .map_err(|error| error.to_string())?
            .len(),
    })
}

#[tauri::command]
async fn account_cancel_switch(state: State<'_, AppState>) -> Result<(), String> {
    account_logout(state).await.map(|_| ())
}

#[tauri::command]
fn account_pending_switch(state: State<'_, AppState>) -> Result<Option<AccountAuthResult>, String> {
    let session = match account_status(state.clone())? {
        AccountStatus::SignedIn(session) | AccountStatus::Expired(session) => session,
        AccountStatus::SignedOut => return Ok(None),
    };
    let result = prepare_account_auth(&state, session)?;
    Ok(result.requires_choice.then_some(result))
}

#[tauri::command]
async fn account_delete(confirmation: String, state: State<'_, AppState>) -> Result<(), String> {
    if confirmation != "DELETE" {
        return Err("注销确认文字不匹配".to_owned());
    }
    let _session = state.sync_lock.lock().await;
    let (base_url, tokens) = {
        let repository = state.repository()?;
        (
            repository
                .cloud_base_url()
                .map_err(|error| error.to_string())?,
            state.accounts.tokens().map_err(|error| error.to_string())?,
        )
    };
    let base_url = base_url.ok_or("云服务地址未配置")?;
    let tokens = tokens.ok_or("当前未登录")?;
    CloudClient::new(&base_url)
        .map_err(|error| error.to_string())?
        .delete_account(&tokens.access_token)
        .await
        .map_err(|error| error.to_string())?;
    state.accounts.sign_out().map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
const UPDATE_TARGET: &str = "windows-x86_64";
#[cfg(target_os = "macos")]
const UPDATE_TARGET: &str = "darwin-aarch64";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const UPDATE_TARGET: &str = "linux-x86_64";

const DEFAULT_CLOUD_BASE_URL: &str = "http://127.0.0.1:8070";

fn platform_base_url(state: &State<'_, AppState>) -> Result<String, String> {
    let configured = option_env!("SPARROWSHELL_API_BASE_URL").unwrap_or(DEFAULT_CLOUD_BASE_URL);
    Ok(state
        .repository()?
        .cloud_base_url()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| configured.trim_end_matches('/').to_owned()))
}

async fn account_access_token(
    state: &State<'_, AppState>,
    client: &CloudClient,
) -> Result<String, String> {
    let tokens = state
        .accounts
        .tokens()
        .map_err(|error| error.to_string())?
        .ok_or("当前未登录")?;
    if tokens.expires_at > chrono::Utc::now() + chrono::Duration::seconds(30) {
        return Ok(tokens.access_token.clone());
    }
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .ok_or("登录状态已失效，请重新登录")?;
    let device_id = uuid::Uuid::parse_str(&tokens.device_id).map_err(|_| "本机设备标识无效")?;
    let refreshed = client
        .refresh(refresh_token, device_id)
        .await
        .map_err(|error| error.to_string())?;
    let access_token = refreshed.access_token.clone();
    state
        .accounts
        .save(refreshed)
        .map_err(|error| error.to_string())?;
    Ok(access_token)
}

#[tauri::command]
async fn account_profile(state: State<'_, AppState>) -> Result<AccountSession, String> {
    let base_url = platform_base_url(&state)?;
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let access_token = account_access_token(&state, &client).await?;
    let profile = client
        .profile(&access_token)
        .await
        .map_err(|error| error.to_string())?;
    state
        .accounts
        .update_profile(profile.email, profile.nickname)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn account_update_nickname(
    nickname: String,
    state: State<'_, AppState>,
) -> Result<AccountSession, String> {
    let nickname = nickname.trim().to_owned();
    if nickname.is_empty() || nickname.chars().count() > 30 {
        return Err("用户名必须为 1～30 个字符".to_owned());
    }
    let _session = state.sync_lock.lock().await;
    let base_url = platform_base_url(&state)?;
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let access_token = account_access_token(&state, &client).await?;
    let profile = client
        .update_nickname(&access_token, &nickname)
        .await
        .map_err(|error| error.to_string())?;
    state
        .accounts
        .update_profile(profile.email, profile.nickname)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn account_change_password(
    current_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let current_password = Zeroizing::new(current_password);
    let new_password = Zeroizing::new(new_password);
    if current_password.is_empty() || !valid_account_password(&new_password) {
        return Err("新密码至少 8 位，必须包含字母及数字或符号".to_owned());
    }
    let _session = state.sync_lock.lock().await;
    let base_url = platform_base_url(&state)?;
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let access_token = account_access_token(&state, &client).await?;
    client
        .change_password(
            &access_token,
            current_password.to_string(),
            new_password.to_string(),
        )
        .await
        .map_err(|error| error.to_string())?;
    state.accounts.sign_out().map_err(|error| error.to_string())
}

#[tauri::command]
async fn check_update(
    include_silent: bool,
    state: State<'_, AppState>,
) -> Result<UpdateCheck, String> {
    let base_url = platform_base_url(&state)?;
    let device_id = state
        .repository()?
        .sync_device_id()
        .map_err(|error| error.to_string())?;
    CloudClient::new(&base_url)
        .map_err(|error| error.to_string())?
        .check_update(
            env!("CARGO_PKG_VERSION"),
            device_id,
            UPDATE_TARGET,
            include_silent,
        )
        .await
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdateDownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn updates_directory(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("updates");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

/// 应用内下载更新安装包：流式写入本地 updates 目录并校验大小与 SHA-256，
/// 校验失败会立即删除文件，避免把损坏或被篡改的安装包交给安装器。
#[tauri::command]
async fn download_update(
    url: String,
    expected_sha256: Option<String>,
    expected_bytes: Option<u64>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|_| "更新下载地址无效".to_owned())?;
    if parsed.scheme() != "https" {
        return Err("更新下载地址必须使用 HTTPS".to_owned());
    }
    let file_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| name.len() <= 128 && name.to_ascii_lowercase().ends_with(".exe"))
        .ok_or("更新安装包文件名无效".to_owned())?
        .to_owned();
    if !file_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err("更新安装包文件名无效".to_owned());
    }
    let directory = updates_directory(&app)?;
    let target_path = directory.join(&file_name);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(parsed)
        .send()
        .await
        .map_err(|_| "无法连接更新服务器".to_owned())?
        .error_for_status()
        .map_err(|_| "更新服务器拒绝了下载请求".to_owned())?;
    let total = response.content_length().or(expected_bytes);
    let mut file = fs::File::create(&target_path).map_err(|error| error.to_string())?;
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|_| "更新下载中断，请稍后重试".to_owned())?;
        let Some(chunk) = chunk else { break };
        hasher.update(&chunk);
        use std::io::Write;
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= std::time::Duration::from_millis(150) {
            last_emit = std::time::Instant::now();
            let _ = app.emit(
                "update-download-progress",
                UpdateDownloadProgress { downloaded, total },
            );
        }
    }
    drop(file);
    let _ = app.emit(
        "update-download-progress",
        UpdateDownloadProgress { downloaded, total },
    );
    if let Some(expected) = expected_bytes.filter(|value| *value > 0) {
        if downloaded != expected {
            let _ = fs::remove_file(&target_path);
            return Err("更新安装包大小不匹配，已终止安装".to_owned());
        }
    }
    if let Some(expected) = expected_sha256
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&target_path);
            return Err("更新安装包校验失败，已终止安装".to_owned());
        }
    }
    Ok(target_path.to_string_lossy().into_owned())
}

/// Windows 应用内更新：退出当前进程，由独立 cmd 脚本静默运行 NSIS 安装器，
/// 完成后自动重新启动应用；安装失败也会回退启动旧版本，避免用户失去可用客户端。
#[tauri::command]
fn install_update_and_restart(path: String, app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (path, app);
        Err("应用内安装更新目前仅支持 Windows 客户端".to_owned())
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // canonicalize() 在 Windows 上返回带 \\?\ 前缀的 verbatim 路径，
        // cmd.exe 无法执行这种路径，写进批处理前必须还原成常规路径。
        fn cmd_safe_path(path: &std::path::Path) -> String {
            let value = path.to_string_lossy();
            value
                .strip_prefix(r"\\?\")
                .map(str::to_owned)
                .unwrap_or_else(|| value.into_owned())
        }
        let updates = updates_directory(&app)?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let installer = std::path::PathBuf::from(&path)
            .canonicalize()
            .map_err(|_| "更新安装包不存在，请重新下载".to_owned())?;
        if !installer.starts_with(&updates)
            || installer.extension().and_then(|value| value.to_str()) != Some("exe")
        {
            return Err("更新安装包路径无效".to_owned());
        }
        let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let script = updates.join("apply-update.cmd");
        let log_path = updates.join("apply-update.log");
        let content = format!(
            "@echo off\r\n\"{}\" /S\r\necho installer_exit=%ERRORLEVEL% > \"{}\"\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
            cmd_safe_path(&installer),
            cmd_safe_path(&log_path),
            cmd_safe_path(&current_exe)
        );
        fs::write(&script, content).map_err(|error| error.to_string())?;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        // 不要手动给脚本路径加引号：Rust 会把参数里的 " 转义成 \"，
        // 而 cmd 不认 MSVCRT 转义，会把 \" 当成文件名的一部分导致脚本根本不执行。
        // 路径含空格时由 Rust 自动加引号，cmd /C 对纯路径会正确剥离外层引号。
        let script_arg = cmd_safe_path(&script);
        std::process::Command::new("cmd")
            .args(["/C", &script_arg])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .map_err(|error| error.to_string())?;
        std::process::exit(0);
    }
}

#[tauri::command]
async fn submit_feedback(
    description: String,
    contact: Option<String>,
    attachment_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<FeedbackReceipt, String> {
    let description = description.trim().to_owned();
    if description.is_empty() || description.len() > 10_000 {
        return Err("请填写问题描述，最多 10000 个字符".to_owned());
    }
    let contact = contact
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if contact.as_ref().is_some_and(|value| value.len() > 320) {
        return Err("联系方式不能超过 320 个字符".to_owned());
    }
    let base_url = platform_base_url(&state)?;
    let client = CloudClient::new(&base_url).map_err(|error| error.to_string())?;
    let tokens = state.accounts.tokens().map_err(|error| error.to_string())?;
    let mut access_token = tokens.as_ref().map(|value| value.access_token.clone());
    if tokens
        .as_ref()
        .is_some_and(|value| value.expires_at <= chrono::Utc::now() + chrono::Duration::seconds(30))
    {
        let current = tokens.as_ref().expect("checked account tokens");
        let refresh_token = current
            .refresh_token
            .as_deref()
            .ok_or("登录状态已失效，请重新登录或退出后匿名提交")?;
        let device_id =
            uuid::Uuid::parse_str(&current.device_id).map_err(|_| "本机设备标识无效")?;
        let refreshed = client
            .refresh(refresh_token, device_id)
            .await
            .map_err(|error| error.to_string())?;
        access_token = Some(refreshed.access_token.clone());
        state
            .accounts
            .save(refreshed)
            .map_err(|error| error.to_string())?;
    }
    let attachment = attachment_path
        .map(|value| {
            let path = std::path::PathBuf::from(value);
            let metadata = fs::metadata(&path).map_err(|_| "无法读取所选附件".to_owned())?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 10 * 1024 * 1024 {
                return Err("附件不能为空且不能超过 10 MB".to_owned());
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("附件文件名无效")?
                .to_owned();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let content_type = match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "pdf" => "application/pdf",
                "txt" | "log" => "text/plain",
                "zip" => "application/zip",
                _ => return Err("仅支持图片、PDF、文本、日志或 ZIP 附件".to_owned()),
            };
            let bytes = fs::read(&path).map_err(|_| "无法读取所选附件".to_owned())?;
            Ok((name, content_type.to_owned(), bytes))
        })
        .transpose()?;
    client
        .submit_feedback(
            access_token.as_deref(),
            &description,
            contact.as_deref(),
            attachment,
        )
        .await
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStatus {
    cursor: u64,
    outbox: usize,
    conflicts: usize,
}

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let repository = state.repository()?;
    Ok(SyncStatus {
        cursor: repository
            .sync_cursor()
            .map_err(|error| error.to_string())?,
        outbox: repository.outbox_len().map_err(|error| error.to_string())?,
        conflicts: repository
            .unresolved_conflicts()
            .map_err(|error| error.to_string())?
            .len(),
    })
}

fn current_sync_account(state: &State<'_, AppState>) -> Result<String, String> {
    let tokens = state
        .accounts
        .tokens()
        .map_err(|error| error.to_string())?
        .ok_or("请先登录云服务")?;
    let owner = state
        .repository()?
        .workspace_owner()
        .map_err(|error| error.to_string())?;
    if owner.as_deref() != Some(tokens.user_id.as_str()) {
        return Err("请先完成账号切换的数据处理选择".to_owned());
    }
    Ok(tokens.user_id.clone())
}

fn run_sync(
    repository: &Repository,
    base_url: &str,
    account_id: &str,
) -> Result<SyncReport, String> {
    if repository
        .workspace_owner()
        .map_err(|error| error.to_string())?
        .as_deref()
        != Some(account_id)
    {
        return Err("同步账号与本机工作区归属不一致，已停止同步".to_owned());
    }
    let account_store = LocalAccountStore::new(
        repository
            .data_directory()
            .ok_or("本地账号存储目录不可用")?,
    );
    let transport =
        HttpSyncTransport::new(base_url, account_store).map_err(|error| error.to_string())?;
    tauri::async_runtime::block_on(SyncEngine::new(repository, &transport).run_once_for(account_id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<SyncReport, String> {
    let _session = state.sync_lock.lock().await;
    let account_id = current_sync_account(&state)?;
    let base_url = state
        .repository()?
        .cloud_base_url()
        .map_err(|error| error.to_string())?
        .ok_or("请先登录云服务")?;
    let repository = state.repository.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let repository = repository
            .lock()
            .map_err(|_| "本地存储暂时不可用".to_owned())?;
        run_sync(&repository, &base_url, &account_id)
    })
    .await
    .map_err(|_| "同步任务已中断".to_owned())?
}

#[tauri::command]
fn sync_conflicts(state: State<'_, AppState>) -> Result<Vec<SyncConflictRecord>, String> {
    state
        .repository()?
        .unresolved_conflicts()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn resolve_sync_conflict(
    conflict_id: String,
    keep_local: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conflict_id = uuid::Uuid::parse_str(&conflict_id).map_err(|_| "冲突标识无效")?;
    let result = if keep_local {
        state
            .repository()?
            .resolve_conflict_keep_local(conflict_id)
            .map_err(|error| error.to_string())?;
        None
    } else {
        Some(
            state
                .repository()?
                .resolve_conflict_keep_remote(conflict_id)
                .map_err(|error| error.to_string())?,
        )
    };
    if let Some(reference) = result.and_then(|result| result.credential_ref) {
        let reference = CredentialRef::parse(reference).map_err(|error| error.to_string())?;
        state
            .credentials
            .delete(&reference)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn monitor_snapshot(
    session_id: String,
    generation: u64,
    state: State<'_, AppState>,
) -> Result<SystemSnapshot, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .monitor_snapshot(session_id, generation)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn terminate_process(
    session_id: String,
    generation: u64,
    pid: u32,
    start_id: String,
    signal: ProcessSignal,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .terminate_process(session_id, generation, pid, start_id, signal)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_remote_directory(
    session_id: String,
    generation: u64,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<RemoteDirectory, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .list_remote_directory(session_id, generation, path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_remote_directory(
    session_id: String,
    generation: u64,
    parent: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .create_remote_directory(session_id, generation, parent, name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_remote_file(
    session_id: String,
    generation: u64,
    parent: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .create_remote_file(session_id, generation, parent, name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn read_remote_text_file(
    session_id: String,
    generation: u64,
    path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .read_remote_text_file(session_id, generation, path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn write_remote_text_file(
    session_id: String,
    generation: u64,
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .write_remote_text_file(session_id, generation, path, content)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_remote_entry(
    session_id: String,
    generation: u64,
    path: String,
    expected_kind: RemoteFileKind,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .remove_remote_entry(session_id, generation, path, expected_kind)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn quick_remove_remote_entry(
    session_id: String,
    generation: u64,
    path: String,
    expected_kind: RemoteFileKind,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .quick_remove_remote_entry(session_id, generation, path, expected_kind)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn rename_remote_entry(
    session_id: String,
    generation: u64,
    path: String,
    new_name: String,
    expected_kind: RemoteFileKind,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .rename_remote_entry(session_id, generation, path, new_name, expected_kind)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn move_remote_entry(
    session_id: String,
    generation: u64,
    path: String,
    destination_directory: String,
    expected_kind: RemoteFileKind,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .move_remote_entry(
            session_id,
            generation,
            path,
            destination_directory,
            expected_kind,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn extract_remote_archive(
    session_id: String,
    generation: u64,
    path: String,
    separate_directory: bool,
    overwrite: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .extract_remote_archive(session_id, generation, path, separate_directory, overwrite)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_upload(
    session_id: String,
    generation: u64,
    local_path: String,
    remote_path: String,
    overwrite: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .start_upload(session_id, generation, local_path, remote_path, overwrite)
        .await
        .map(|id| id.to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_download(
    session_id: String,
    generation: u64,
    remote_path: String,
    local_path: String,
    overwrite: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .start_download(session_id, generation, remote_path, local_path, overwrite)
        .await
        .map(|id| id.to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_transfer(
    transfer_id: String,
    session_id: String,
    generation: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let transfer_id = TransferId::from_str(&transfer_id).map_err(|error| error.to_string())?;
    let session_id = SessionId::from_str(&session_id).map_err(|error| error.to_string())?;
    state
        .sessions
        .cancel_transfer(transfer_id, session_id, generation)
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AiProposalResult {
    summary: String,
    plan: AiPlan,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AiAnalysisResult {
    execution_id: String,
    analysis: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct AiContextPermissions {
    server_label: bool,
    system_info: bool,
    working_directory: bool,
    selected_output: bool,
    recent_results: bool,
    directory_entries: bool,
}

fn build_ai_context(
    target: TargetRef,
    mut input: ContextInput,
    permissions: &AiContextPermissions,
    trusted_results: Option<Vec<String>>,
) -> Result<String, String> {
    if let Some(results) = trusted_results {
        input.recent_results = results;
    }
    let mut context = ContextBuilder::new(target);
    let grants = [
        (permissions.server_label, ContextCategory::ServerLabel),
        (permissions.system_info, ContextCategory::SystemInfo),
        (
            permissions.working_directory,
            ContextCategory::WorkingDirectory,
        ),
        (permissions.selected_output, ContextCategory::SelectedOutput),
        (permissions.recent_results, ContextCategory::RecentResults),
        (
            permissions.directory_entries,
            ContextCategory::DirectoryEntries,
        ),
    ];
    for (granted, category) in grants {
        if granted {
            context.authorize(category);
        }
    }
    context
        .build(target, input)
        .and_then(|context| context.to_request_json(target))
        .map_err(|error| error.to_string())
}

fn trusted_execution_result(
    record: &sparrowshell_execution::ExecutionRecord,
    target: TargetRef,
) -> Result<String, String> {
    if record.target != target {
        return Err("执行记录与当前服务器或会话不一致".to_owned());
    }
    if !matches!(
        record.state,
        ExecutionState::Succeeded | ExecutionState::Failed
    ) {
        return Err("只能使用已成功或失败的真实执行结果".to_owned());
    }
    Ok(serde_json::json!({
        "executionId": record.execution_id,
        "command": record.command,
        "state": record.state,
        "exitStatus": record.exit_status,
        "stdout": record.stdout,
        "stderr": record.stderr,
        "errorCode": record.error_code,
    })
    .to_string())
}

#[derive(serde::Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum AiAgentTurnResult {
    ToolCall {
        summary: String,
        plan: AiPlan,
        session_id: String,
    },
    FinalAnswer {
        answer: String,
        session_id: String,
    },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentTurnCommandInput {
    intent: String,
    task_id: String,
    history: Vec<ConversationMessage>,
    locale: String,
    target: TargetRef,
    input: ContextInput,
    permissions: AiContextPermissions,
    execution_ids: Vec<String>,
    session_id: Option<String>,
}

fn installation_id(state: &State<'_, AppState>) -> Result<uuid::Uuid, String> {
    state
        .repository()?
        .sync_device_id()
        .map_err(|error| error.to_string())
}

fn license_refs(device_id: uuid::Uuid) -> (CredentialRef, CredentialRef) {
    (
        CredentialRef::parse(format!("license/{device_id}")).expect("validated license reference"),
        CredentialRef::parse(format!("license-device/{device_id}"))
            .expect("validated license device reference"),
    )
}

fn remove_legacy_license_secrets(state: &State<'_, AppState>, device_id: uuid::Uuid) {
    let (license_reference, device_reference) = license_refs(device_id);
    let _ = state.credentials.delete(&license_reference);
    let _ = state.credentials.delete(&device_reference);
}

fn stored_license_key(state: &State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .repository()?
        .ai_license_key()
        .map_err(|error| error.to_string())
}

fn stored_device_token(state: &State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .repository()?
        .ai_device_token()
        .map_err(|error| error.to_string())
}

fn billing_client(state: &State<'_, AppState>) -> Result<BillingClient, String> {
    BillingClient::new(
        installation_id(state)?.to_string(),
        stored_device_token(state)?,
    )
    .map_err(|error| error.to_string())
}

fn identified_gateway(state: &State<'_, AppState>) -> Result<AiGateway, String> {
    let anonymous_id = installation_id(state)?;
    let access_token = state
        .accounts
        .tokens()
        .map_err(|error| error.to_string())?
        .map(|tokens| tokens.access_token.clone());
    let identity = sparrowshell_ai_client::gateway::GatewayIdentity::with_device_token(
        access_token,
        anonymous_id.to_string(),
        stored_device_token(state)?,
    )
    .map_err(|error| error.to_string())?;
    Ok(state.gateway.with_identity(identity))
}

#[tauri::command]
async fn ai_entitlement(state: State<'_, AppState>) -> Result<AiEntitlement, String> {
    billing_client(&state)?
        .entitlement()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_license_key(state: State<'_, AppState>) -> Result<Option<String>, String> {
    if let Some(license_key) = stored_license_key(&state)? {
        return Ok(Some(license_key));
    }
    // Older clients kept only the device token outside Keychain. Recover that
    // installation's key once, then store it locally without using Keychain.
    let license_key = billing_client(&state)?
        .recover()
        .await
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_ai_license_key(&license_key)
        .map_err(|error| error.to_string())?;
    Ok(Some(license_key))
}

#[tauri::command]
async fn ai_create_order(plan: String, state: State<'_, AppState>) -> Result<BillingOrder, String> {
    let license_key = stored_license_key(&state)?;
    billing_client(&state)?
        .create_order(&plan, license_key.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_activate_license(
    license_key: String,
    state: State<'_, AppState>,
) -> Result<AiEntitlement, String> {
    let license_key = license_key.trim().to_uppercase();
    let activation = billing_client(&state)?
        .activate(&license_key)
        .await
        .map_err(|error| error.to_string())?;
    let device_id = installation_id(&state)?;
    state
        .repository()?
        .save_ai_license(&license_key, &activation.device_token)
        .map_err(|error| error.to_string())?;
    remove_legacy_license_secrets(&state, device_id);
    Ok(activation.entitlement)
}

#[tauri::command]
async fn ai_deactivate_license(state: State<'_, AppState>) -> Result<AiEntitlement, String> {
    let entitlement = billing_client(&state)?
        .deactivate()
        .await
        .map_err(|error| error.to_string())?;
    let device_id = installation_id(&state)?;
    state
        .repository()?
        .clear_ai_license()
        .map_err(|error| error.to_string())?;
    remove_legacy_license_secrets(&state, device_id);
    Ok(entitlement)
}

#[tauri::command]
async fn ai_release_task(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    billing_client(&state)?
        .release_task(&task_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_agent_turn(
    request: AgentTurnCommandInput,
    state: State<'_, AppState>,
) -> Result<AiAgentTurnResult, String> {
    let AgentTurnCommandInput {
        intent,
        task_id,
        history,
        locale,
        target,
        input,
        permissions,
        execution_ids,
        session_id,
    } = request;
    if execution_ids.len() > 5
        || execution_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != execution_ids.len()
    {
        return Err("动态 Agent 最多使用 5 条不重复的真实执行记录".to_owned());
    }
    if !execution_ids.is_empty() && !permissions.recent_results {
        return Err("必须明确授权最近执行结果后才能继续动态分析".to_owned());
    }
    let mut results = Vec::with_capacity(execution_ids.len());
    for execution_id in execution_ids {
        let execution_id =
            ExecutionId::from_str(&execution_id).map_err(|error| error.to_string())?;
        let record = state
            .ai
            .broker()
            .record(execution_id)
            .ok_or("找不到这条真实执行记录")?;
        results.push(trusted_execution_result(&record, target)?);
    }
    let context = build_ai_context(target, input, &permissions, Some(results))?;
    match identified_gateway(&state)?
        .agent_turn(&intent, history, &locale, session_id, context, task_id)
        .await
        .map_err(|error| error.to_string())?
    {
        AgentTurnResult::ToolCall {
            summary,
            proposal,
            session_id,
        } => {
            let plan = state
                .ai
                .create_plan(target, vec![proposal])
                .map_err(|error| error.to_string())?;
            Ok(AiAgentTurnResult::ToolCall {
                summary,
                plan,
                session_id,
            })
        }
        AgentTurnResult::FinalAnswer { answer, session_id } => {
            Ok(AiAgentTurnResult::FinalAnswer { answer, session_id })
        }
    }
}

#[tauri::command]
async fn ai_propose(
    intent: String,
    locale: String,
    target: TargetRef,
    input: ContextInput,
    permissions: AiContextPermissions,
    state: State<'_, AppState>,
) -> Result<AiProposalResult, String> {
    let context = build_ai_context(target, input, &permissions, None)?;
    let (summary, proposals) = identified_gateway(&state)?
        .propose(&intent, &locale, context, 3)
        .await
        .map_err(|error| error.to_string())?;
    let plan = state
        .ai
        .create_plan(target, proposals)
        .map_err(|error| error.to_string())?;
    Ok(AiProposalResult { summary, plan })
}

#[tauri::command]
async fn ai_analyze(
    execution_id: String,
    locale: String,
    target: TargetRef,
    authorize_recent_results: bool,
    state: State<'_, AppState>,
) -> Result<AiAnalysisResult, String> {
    if !authorize_recent_results {
        return Err("必须明确授权最近执行结果后才能进行 AI 分析".to_owned());
    }
    let execution_id = ExecutionId::from_str(&execution_id).map_err(|error| error.to_string())?;
    let record = state
        .ai
        .broker()
        .record(execution_id)
        .ok_or("找不到这条真实执行记录")?;
    if record.target != target {
        return Err("执行记录与当前服务器或会话不一致".to_owned());
    }
    if !matches!(
        record.state,
        ExecutionState::Succeeded | ExecutionState::Failed
    ) {
        return Err("只能分析已成功或失败的真实执行结果".to_owned());
    }
    let result = serde_json::json!({
        "command": record.command,
        "state": record.state,
        "exitStatus": record.exit_status,
        "stdout": record.stdout,
        "stderr": record.stderr,
    })
    .to_string();
    let mut context = ContextBuilder::new(target);
    context.authorize(ContextCategory::RecentResults);
    let context = context
        .build(
            target,
            ContextInput {
                recent_results: vec![result],
                ..ContextInput::default()
            },
        )
        .and_then(|context| context.to_request_json(target))
        .map_err(|error| error.to_string())?;
    let analysis = identified_gateway(&state)?
        .analyze(&locale, context)
        .await
        .map_err(|error| error.to_string())?;
    Ok(AiAnalysisResult {
        execution_id: execution_id.to_string(),
        analysis,
    })
}

#[tauri::command]
fn ai_create_plan(
    target: TargetRef,
    proposals: Vec<CommandProposal>,
    state: State<'_, AppState>,
) -> Result<AiPlan, String> {
    state
        .ai
        .create_plan(target, proposals)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_run_next(
    plan_id: String,
    target: TargetRef,
    mode: ExecutionMode,
    state: State<'_, AppState>,
) -> Result<StepProgress, String> {
    let plan_id = PlanId::from_str(&plan_id).map_err(|error| error.to_string())?;
    let orchestrator = state.ai.clone();
    let sessions = state.sessions.clone();
    orchestrator
        .run_next(plan_id, target, mode, &sessions)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_approve_and_run(
    plan_id: String,
    approval_id: String,
    command: String,
    target: TargetRef,
    state: State<'_, AppState>,
) -> Result<StepProgress, String> {
    let plan_id = PlanId::from_str(&plan_id).map_err(|error| error.to_string())?;
    let approval_id = ApprovalId::from_str(&approval_id).map_err(|error| error.to_string())?;
    let orchestrator = state.ai.clone();
    let sessions = state.sessions.clone();
    orchestrator
        .approve_and_run(
            plan_id,
            approval_id,
            &CommandHash::from_command(&command),
            target,
            &sessions,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_create(spec: BatchSpec, state: State<'_, AppState>) -> Result<String, String> {
    let id = state
        .batch
        .create(spec)
        .map_err(|error| error.to_string())?;
    let run = state.batch.snapshot(id).ok_or("批量任务不存在")?;
    state
        .repository()?
        .save_automation_run(
            "batch",
            serde_json::to_value(run).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    Ok(id.to_string())
}

#[tauri::command]
fn batch_snapshot(run_id: String, state: State<'_, AppState>) -> Result<BatchRun, String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    state
        .batch
        .snapshot(id)
        .ok_or_else(|| "批量任务不存在".to_owned())
}

#[tauri::command]
async fn batch_run(run_id: String, state: State<'_, AppState>) -> Result<BatchRun, String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    let result = state
        .batch
        .run(
            id,
            state.ai.broker().clone(),
            Arc::new(state.sessions.clone()),
        )
        .await
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_automation_run(
            "batch",
            serde_json::to_value(&result).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
fn batch_stop(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    state
        .batch
        .request_stop(id, state.ai.broker())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn workflow_create(version: WorkflowVersion, state: State<'_, AppState>) -> Result<String, String> {
    let id = state
        .workflows
        .start(version, ExecutionMode::ConfirmEach)
        .map_err(|error| error.to_string())?;
    let (sender, _receiver) = tokio::sync::watch::channel(false);
    state
        .workflow_cancellations
        .lock()
        .map_err(|_| "流程状态不可用".to_owned())?
        .insert(id, sender);
    let run = state.workflows.snapshot(id).ok_or("流程不存在")?;
    state
        .repository()?
        .save_automation_run(
            "workflow",
            serde_json::to_value(run).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    Ok(id.to_string())
}

#[tauri::command]
async fn workflow_run(
    run_id: String,
    mode: ExecutionMode,
    state: State<'_, AppState>,
) -> Result<WorkflowExecution, String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    let receiver = state
        .workflow_cancellations
        .lock()
        .map_err(|_| "流程状态不可用".to_owned())?
        .get(&id)
        .cloned()
        .ok_or("流程不存在或已经结束".to_owned())?
        .subscribe();
    let result = state
        .workflows
        .run(id, state.ai.broker(), &state.sessions, mode, receiver)
        .await
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_automation_run(
            "workflow",
            serde_json::to_value(&result.run).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    if result.run.state != WorkflowRunState::AwaitingApproval {
        state
            .workflow_cancellations
            .lock()
            .map_err(|_| "流程状态不可用".to_owned())?
            .remove(&id);
    }
    Ok(result)
}

#[tauri::command]
async fn workflow_approve_and_run(
    run_id: String,
    approval_id: String,
    command: String,
    mode: ExecutionMode,
    state: State<'_, AppState>,
) -> Result<WorkflowExecution, String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    let approval_id = ApprovalId::from_str(&approval_id).map_err(|error| error.to_string())?;
    let receiver = state
        .workflow_cancellations
        .lock()
        .map_err(|_| "流程状态不可用".to_owned())?
        .get(&id)
        .cloned()
        .ok_or("流程不存在或已经结束".to_owned())?
        .subscribe();
    let broker = state.ai.broker().clone();
    let result = state
        .workflows
        .approve_and_run(
            id,
            approval_id,
            CommandHash::from_command(&command),
            WorkflowRunContext {
                broker: &broker,
                executor: &state.sessions,
                mode,
                cancel: receiver,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_automation_run(
            "workflow",
            serde_json::to_value(&result.run).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    if result.run.state != WorkflowRunState::AwaitingApproval {
        state
            .workflow_cancellations
            .lock()
            .map_err(|_| "流程状态不可用".to_owned())?
            .remove(&id);
    }
    Ok(result)
}

#[tauri::command]
fn workflow_stop(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&run_id).map_err(|error| error.to_string())?;
    let sender = state
        .workflow_cancellations
        .lock()
        .map_err(|_| "流程状态不可用".to_owned())?
        .get(&id)
        .cloned()
        .ok_or("流程不存在或已经结束".to_owned())?;
    sender.send(true).map_err(|error| error.to_string())?;
    if state.workflows.snapshot(id).map(|run| run.state) == Some(WorkflowRunState::AwaitingApproval)
    {
        let stopped = state
            .workflows
            .stop_awaiting(id)
            .map_err(|error| error.to_string())?;
        state
            .repository()?
            .save_automation_run(
                "workflow",
                serde_json::to_value(stopped.run).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        state
            .workflow_cancellations
            .lock()
            .map_err(|_| "流程状态不可用".to_owned())?
            .remove(&id);
    }
    Ok(())
}

fn automation_entity_type(kind: &str) -> Result<SyncEntityType, String> {
    match kind {
        "batch_template" => Ok(SyncEntityType::BatchTemplate),
        "workflow_definition" => Ok(SyncEntityType::WorkflowDefinition),
        _ => Err("不支持的自动化定义类型".to_owned()),
    }
}

#[tauri::command]
fn automation_definition_save(
    kind: String,
    id: Option<String>,
    name: String,
    definition: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<AutomationDefinitionSyncDto, String> {
    let id = id
        .map(|value| uuid::Uuid::parse_str(&value))
        .transpose()
        .map_err(|error| error.to_string())?;
    state
        .repository()?
        .save_automation_definition(automation_entity_type(&kind)?, id, &name, definition)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn automation_definitions(
    kind: String,
    state: State<'_, AppState>,
) -> Result<Vec<AutomationDefinitionSyncDto>, String> {
    state
        .repository()?
        .automation_definitions(automation_entity_type(&kind)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn automation_definition_delete(
    kind: String,
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| error.to_string())?;
    state
        .repository()?
        .delete_automation_definition(automation_entity_type(&kind)?, id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn automation_run_history(
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<AutomationRunRecord>, String> {
    state
        .repository()?
        .automation_runs(limit.unwrap_or(50))
        .map_err(|error| error.to_string())
}

#[cfg(debug_assertions)]
static SYNTHETIC_EXECUTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(debug_assertions)]
struct SyntheticExecutor;

#[cfg(debug_assertions)]
#[async_trait::async_trait]
impl sparrowshell_execution::CommandExecutor for SyntheticExecutor {
    async fn execute(
        &self,
        _permit: &sparrowshell_execution::ExecutionPermit,
        _command: &str,
    ) -> Result<sparrowshell_execution::RemoteCommandResult, sparrowshell_execution::ExecutorError>
    {
        SYNTHETIC_EXECUTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(sparrowshell_execution::RemoteCommandResult {
            stdout: "synthetic stdout: checked".to_owned(),
            stderr: "synthetic stderr: missing file".to_owned(),
            exit_status: Some(1),
        })
    }
}

#[cfg(debug_assertions)]
fn activate_synthetic_target(state: &State<'_, AppState>, target: TargetRef) -> Result<(), String> {
    state
        .ai
        .broker()
        .activate_target(target)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn ai_e2e_execution_count() -> Result<usize, String> {
    #[cfg(not(debug_assertions))]
    {
        Err("原生 UI 验收命令仅在开发构建中可用".to_owned())
    }
    #[cfg(debug_assertions)]
    {
        Ok(SYNTHETIC_EXECUTIONS.load(std::sync::atomic::Ordering::SeqCst))
    }
}

#[tauri::command]
async fn ai_e2e_run_next(
    plan_id: String,
    target: TargetRef,
    mode: ExecutionMode,
    state: State<'_, AppState>,
) -> Result<StepProgress, String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (plan_id, target, mode, state);
        Err("原生 UI 验收命令仅在开发构建中可用".to_owned())
    }
    #[cfg(debug_assertions)]
    {
        let plan_id = PlanId::from_str(&plan_id).map_err(|error| error.to_string())?;
        activate_synthetic_target(&state, target)?;
        state
            .ai
            .run_next(plan_id, target, mode, &SyntheticExecutor)
            .await
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
async fn ai_e2e_approve_and_run(
    plan_id: String,
    approval_id: String,
    command: String,
    target: TargetRef,
    state: State<'_, AppState>,
) -> Result<StepProgress, String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (plan_id, approval_id, command, target, state);
        Err("原生 UI 验收命令仅在开发构建中可用".to_owned())
    }
    #[cfg(debug_assertions)]
    {
        let plan_id = PlanId::from_str(&plan_id).map_err(|error| error.to_string())?;
        let approval_id = ApprovalId::from_str(&approval_id).map_err(|error| error.to_string())?;
        activate_synthetic_target(&state, target)?;
        state
            .ai
            .approve_and_run(
                plan_id,
                approval_id,
                &CommandHash::from_command(&command),
                target,
                &SyntheticExecutor,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn ai_e2e_report(report: String) -> Result<(), String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = report;
        Err("原生 UI 验收报告仅在开发构建中可用".to_owned())
    }
    #[cfg(debug_assertions)]
    {
        if report.len() > 64 * 1024 {
            return Err("验收报告过大".to_owned());
        }
        let path =
            std::env::var_os("SPARROWSHELL_E2E_REPORT").ok_or("未配置原生 UI 验收报告路径")?;
        fs::write(path, report).map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn ai_stop(plan_id: String, state: State<'_, AppState>) -> Result<AiPlan, String> {
    let plan_id = PlanId::from_str(&plan_id).map_err(|error| error.to_string())?;
    state.ai.stop(plan_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn ai_disconnected(target: TargetRef, state: State<'_, AppState>) -> Result<(), String> {
    state
        .ai
        .disconnected(target)
        .map_err(|error| error.to_string())
}

fn spawn_auto_sync(
    app: tauri::AppHandle,
    repository: Arc<Mutex<Repository>>,
    sync_lock: Arc<tokio::sync::Mutex<()>>,
    auto_sync: Arc<Mutex<AutoSyncState>>,
) {
    #[cfg(debug_assertions)]
    if std::env::var_os("SPARROWSHELL_E2E_DATA_DIR").is_some() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let due = auto_sync
                .lock()
                .map(|state| {
                    state
                        .next_attempt
                        .is_none_or(|at| at <= std::time::Instant::now())
                })
                .unwrap_or(false);
            if !due {
                continue;
            }
            let account_id = app
                .state::<AppState>()
                .accounts
                .tokens()
                .ok()
                .flatten()
                .map(|tokens| tokens.user_id.clone());
            let sync_context = repository.lock().ok().and_then(|repository| {
                let base_url = repository.cloud_base_url().ok().flatten();
                let owner = repository.workspace_owner().ok().flatten();
                match (base_url, owner, account_id) {
                    (Some(base_url), Some(owner), Some(account_id)) if owner == account_id => {
                        Some((base_url, account_id))
                    }
                    _ => None,
                }
            });
            let Some((base_url, account_id)) = sync_context else {
                if let Ok(mut state) = auto_sync.lock() {
                    state.next_attempt = None;
                }
                continue;
            };
            let result = {
                let repository = repository.clone();
                let _session = sync_lock.lock().await;
                tauri::async_runtime::spawn_blocking(move || {
                    let repository = repository
                        .lock()
                        .map_err(|_| "本地存储暂时不可用".to_owned())?;
                    run_sync(&repository, &base_url, &account_id)
                })
                .await
                .map_err(|_| "自动同步任务已中断".to_owned())
                .and_then(|result| result)
            };
            match result {
                Ok(report) => {
                    if let Ok(mut state) = auto_sync.lock() {
                        state.failures = 0;
                        state.next_attempt =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
                    }
                    let _ = app.emit("sync-completed", report);
                }
                Err(error) => {
                    let _ = app.emit("sync-failed", error);
                    if let Ok(mut state) = auto_sync.lock() {
                        state.failures = state.failures.saturating_add(1).min(6);
                        let seconds = 15u64.saturating_mul(1 << (state.failures - 1)).min(240);
                        let jitter = (uuid::Uuid::new_v4().as_u128() % 5_000) as u64;
                        state.next_attempt = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(seconds)
                                + std::time::Duration::from_millis(jitter),
                        );
                    }
                }
            }
        }
    });
}

fn recover_interrupted_automation(repository: &Repository) -> Result<(), String> {
    for record in repository
        .automation_runs(500)
        .map_err(|error| error.to_string())?
    {
        match record.kind.as_str() {
            "batch" => {
                let mut run: BatchRun =
                    serde_json::from_value(record.payload).map_err(|error| error.to_string())?;
                if matches!(run.state, BatchState::Queued | BatchState::Running) {
                    run.mark_interrupted_after_restart();
                    repository
                        .save_automation_run(
                            "batch",
                            serde_json::to_value(run).map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            "workflow" => {
                let mut run: WorkflowRun =
                    serde_json::from_value(record.payload).map_err(|error| error.to_string())?;
                if matches!(
                    run.state,
                    WorkflowRunState::Running | WorkflowRunState::AwaitingApproval
                ) {
                    run.mark_interrupted_after_restart();
                    repository
                        .save_automation_run(
                            "workflow",
                            serde_json::to_value(run).map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            _ => return Err("未知自动化历史类型".to_owned()),
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(native_menu::handle_event)
        .setup(|app| {
            native_menu::install(app.handle())?;
            // 主窗口关闭后立即退出整个进程。
            // 窗口消失但进程残留会让安装/卸载程序无法终止应用（NSIS 报「无法终止」），
            // SSH 会话、SQLite 等均为事务化/可恢复设计，直接退出不会损坏本地数据。
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(|event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        std::process::exit(0);
                    }
                });
            }
            let directory = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            #[cfg(debug_assertions)]
            let directory = std::env::var_os("SPARROWSHELL_E2E_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| directory.join("development"));
            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let repository = Arc::new(Mutex::new(
                Repository::open(directory.join("sparrowshell.db"))
                    .map_err(|error| error.to_string())?,
            ));
            {
                let guard = repository
                    .lock()
                    .map_err(|_| "本地存储暂时不可用".to_owned())?;
                recover_interrupted_automation(&guard)?;
            }
            let credentials = PlatformCredentialStore::for_app(&directory);
            let sessions = SessionManager::new(
                repository.clone(),
                Arc::new(credentials.clone()),
                Arc::new(TauriEvents(app.handle().clone())),
            );
            let sync_lock = Arc::new(tokio::sync::Mutex::new(()));
            let auto_sync = Arc::new(Mutex::new(AutoSyncState::default()));
            app.manage(AppState {
                repository: repository.clone(),
                credentials,
                accounts: AccountTokenStore::new(LocalAccountStore::new(&directory))
                    .map_err(|error| error.to_string())?,
                sessions,
                ai: AiOrchestrator::new(sparrowshell_execution::ExecutionBroker::new()),
                gateway: AiGateway::new().map_err(|error| error.to_string())?,
                batch: BatchEngine::default(),
                workflows: WorkflowEngine::default(),
                workflow_cancellations: Mutex::new(std::collections::HashMap::new()),
                sync_lock: sync_lock.clone(),
            });
            spawn_auto_sync(app.handle().clone(), repository, sync_lock, auto_sync);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            native_appearance::set_app_theme,
            native_menu::set_app_locale,
            read_clipboard,
            catalog_snapshot,
            create_folder,
            save_folder,
            reorder_folders,
            reorder_servers,
            save_server,
            save_password,
            delete_server,
            duplicate_server,
            delete_folder,
            connect,
            test_server_connection,
            trust_host_key,
            terminal_write,
            terminal_resize,
            disconnect,
            close_session,
            record_command,
            command_history,
            clear_command_history,
            account_status,
            account_login,
            account_send_verification_code,
            account_register,
            account_reset_password,
            account_profile,
            account_update_nickname,
            account_change_password,
            account_logout,
            account_switch,
            account_cancel_switch,
            account_pending_switch,
            account_delete,
            check_update,
            download_update,
            install_update_and_restart,
            submit_feedback,
            sync_status,
            sync_now,
            sync_conflicts,
            resolve_sync_conflict,
            monitor_snapshot,
            terminate_process,
            list_remote_directory,
            create_remote_directory,
            create_remote_file,
            read_remote_text_file,
            write_remote_text_file,
            remove_remote_entry,
            quick_remove_remote_entry,
            rename_remote_entry,
            move_remote_entry,
            extract_remote_archive,
            start_upload,
            start_download,
            cancel_transfer,
            ai_entitlement,
            ai_license_key,
            ai_create_order,
            ai_activate_license,
            ai_deactivate_license,
            ai_release_task,
            ai_propose,
            ai_analyze,
            ai_agent_turn,
            ai_create_plan,
            ai_run_next,
            ai_approve_and_run,
            ai_e2e_execution_count,
            ai_e2e_run_next,
            ai_e2e_approve_and_run,
            ai_e2e_report,
            ai_stop,
            ai_disconnected,
            batch_create,
            batch_snapshot,
            batch_run,
            batch_stop,
            workflow_create,
            workflow_run,
            workflow_approve_and_run,
            workflow_stop,
            automation_definition_save,
            automation_definitions,
            automation_definition_delete,
            automation_run_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running SparrowShell");
}

#[cfg(test)]
mod tests {
    use super::UPDATE_TARGET;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_update_target_cannot_use_mac_artifacts() {
        assert_eq!(UPDATE_TARGET, "windows-x86_64");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_update_target_remains_darwin_aarch64() {
        assert_eq!(UPDATE_TARGET, "darwin-aarch64");
    }
}
