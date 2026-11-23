import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { AccountAuthResult, AccountSession, AccountStatus, AccountSwitchChoice, AccountSwitchResult, AiAgentTurnResult, AiBillingOrder, AiEntitlement, AiContextData, AiContextPermissions, AiConversationMessage, AiPlan, CatalogOrderItem, CatalogSnapshot, CommandHistoryEntry, CommandProposal, ConnectResult, ExecutionMode, FeedbackReceipt, Folder, ProcessSignal, RemoteDirectory, RemoteFileKind, ServerInput, ServerRecord, SessionSnapshot, StepProgress, SyncConflict, SyncReport, SyncStatus, SystemSnapshot, TargetRef, TerminalEvent, TestConnectionResult, TransferEvent, UpdateCheck } from '../types';

import type { AutomationDefinition, AutomationDefinitionKind, AutomationRunRecord, BatchRun, BatchSpec, WorkflowExecution, WorkflowVersion } from '../types';

export const isDesktop = '__TAURI_INTERNALS__' in window;
export class DesktopUnavailableError extends Error {
  constructor() { super('真实 SSH 仅可在 SparrowShell macOS 客户端中使用'); this.name = 'DesktopUnavailableError'; }
}

function required<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop) return Promise.reject(new DesktopUnavailableError());
  return invoke<T>(command, args);
}

export const desktop = {
  catalog: () => isDesktop ? invoke<CatalogSnapshot>('catalog_snapshot') : Promise.resolve({ folders: [], servers: [] }),
  createFolder: (parentId: string | null, name: string) => required<Folder>('create_folder', { parentId, name }),
  saveFolder: (id: string, parentId: string | null, name: string) => required<Folder>('save_folder', { id, parentId, name }),
  reorderFolders: (items: CatalogOrderItem[]) => required<void>('reorder_folders', { items }),
  reorderServers: (items: CatalogOrderItem[]) => required<void>('reorder_servers', { items }),
  deleteFolder: (folderId: string) => required<void>('delete_folder', { folderId }),
  saveServer: (input: ServerInput) => required<ServerRecord>('save_server', { input }),
  duplicateServer: (serverId: string) => required<ServerRecord>('duplicate_server', { serverId }),
  savePassword: (serverId: string, password: string) => required<void>('save_password', { serverId, password }),
  deleteServer: (serverId: string) => required<void>('delete_server', { serverId }),
  connect: (serverId: string, sessionId: string) => required<ConnectResult>('connect', { serverId, sessionId }),
  testConnection: (input: ServerInput, secret: string | null, acceptedHostKey: TestConnectionResult['hostKeyChallenge']) => required<TestConnectionResult>('test_server_connection', { input, secret, acceptedHostKey }),
  trustHostKey: (serverId: string, sessionId: string, replace: boolean) => required<ConnectResult>('trust_host_key', { serverId, sessionId, replace }),
  disconnect: (sessionId: string) => required<void>('disconnect', { sessionId }),
  closeSession: (sessionId: string) => required<void>('close_session', { sessionId }),
  recordCommand: (serverId: string, command: string) => required<CommandHistoryEntry | null>('record_command', { serverId, command }),
  commandHistory: (serverId: string | null, limit = 100) => required<CommandHistoryEntry[]>('command_history', { serverId, limit }),
  clearCommandHistory: (serverId: string | null) => required<number>('clear_command_history', { serverId }),
  accountStatus: () => required<AccountStatus>('account_status'),
  accountLogin: (email: string, password: string) => required<AccountAuthResult>('account_login', { email, password }),
  accountSendVerificationCode: (email: string, purpose: 'register' | 'password_reset', locale: 'zh' | 'en') => required<void>('account_send_verification_code', { email, purpose, locale: locale === 'zh' ? 'zh-CN' : 'en-US' }),
  accountRegister: (email: string, password: string, code: string) => required<AccountAuthResult>('account_register', { email, password, code }),
  accountResetPassword: (email: string, code: string, newPassword: string) => required<void>('account_reset_password', { email, code, newPassword }),
  accountProfile: () => required<AccountSession>('account_profile'),
  accountUpdateNickname: (nickname: string) => required<AccountSession>('account_update_nickname', { nickname }),
  accountChangePassword: (currentPassword: string, newPassword: string) => required<void>('account_change_password', { currentPassword, newPassword }),
  accountLogout: () => required<{ remoteRevoked: boolean }>('account_logout'),
  accountSwitch: (choice: AccountSwitchChoice) => required<AccountSwitchResult>('account_switch', { choice }),
  accountCancelSwitch: () => required<void>('account_cancel_switch'),
  accountPendingSwitch: () => required<AccountAuthResult | null>('account_pending_switch'),
  accountDelete: (confirmation: string) => required<void>('account_delete', { confirmation }),
  checkUpdate: (includeSilent = false) => required<UpdateCheck>('check_update', { includeSilent }),
  submitFeedback: (description: string, contact: string, attachmentPath: string | null) => required<FeedbackReceipt>('submit_feedback', { description, contact: contact || null, attachmentPath }),
  openExternal: (url: string) => isDesktop ? openUrl(url) : Promise.reject(new DesktopUnavailableError()),
  syncStatus: () => required<SyncStatus>('sync_status'),
  syncNow: () => required<SyncReport>('sync_now'),
  syncConflicts: () => required<SyncConflict[]>('sync_conflicts'),
  resolveSyncConflict: (conflictId: string, keepLocal: boolean) => required<void>('resolve_sync_conflict', { conflictId, keepLocal }),
  readClipboard: () => required<string>('read_clipboard'),
  setAppLocale: (locale: 'zh' | 'en') => isDesktop ? invoke<void>('set_app_locale', { locale }) : Promise.resolve(),
  setAppTheme: (theme: 'dark' | 'light') => isDesktop ? invoke<void>('set_app_theme', { theme }) : Promise.resolve(),
  onNativeMenu: (handler: (action: 'settings' | 'about') => void) => isDesktop ? listen<'settings' | 'about'>('native-menu-action', ({ payload }) => handler(payload)) : Promise.resolve(() => {}),
  terminalWrite: (sessionId: string, generation: number, data: number[]) => required<void>('terminal_write', { sessionId, generation, data }),
  terminalResize: (sessionId: string, generation: number, cols: number, rows: number) => required<void>('terminal_resize', { sessionId, generation, cols, rows }),
  monitorSnapshot: (sessionId: string, generation: number) => required<SystemSnapshot>('monitor_snapshot', { sessionId, generation }),
  terminateProcess: (sessionId: string, generation: number, pid: number, startId: string, signal: ProcessSignal) => required<void>('terminate_process', { sessionId, generation, pid, startId, signal }),
  listRemoteDirectory: (sessionId: string, generation: number, path: string | null) => required<RemoteDirectory>('list_remote_directory', { sessionId, generation, path }),
  createRemoteDirectory: (sessionId: string, generation: number, parent: string, name: string) => required<void>('create_remote_directory', { sessionId, generation, parent, name }),
  createRemoteFile: (sessionId: string, generation: number, parent: string, name: string) => required<void>('create_remote_file', { sessionId, generation, parent, name }),
  readRemoteTextFile: (sessionId: string, generation: number, path: string) => required<string>('read_remote_text_file', { sessionId, generation, path }),
  writeRemoteTextFile: (sessionId: string, generation: number, path: string, content: string) => required<void>('write_remote_text_file', { sessionId, generation, path, content }),
  removeRemoteEntry: (sessionId: string, generation: number, path: string, expectedKind: RemoteFileKind) => required<void>('remove_remote_entry', { sessionId, generation, path, expectedKind }),
  quickRemoveRemoteEntry: (sessionId: string, generation: number, path: string, expectedKind: RemoteFileKind) => required<void>('quick_remove_remote_entry', { sessionId, generation, path, expectedKind }),
  renameRemoteEntry: (sessionId: string, generation: number, path: string, newName: string, expectedKind: RemoteFileKind) => required<void>('rename_remote_entry', { sessionId, generation, path, newName, expectedKind }),
  moveRemoteEntry: (sessionId: string, generation: number, path: string, destinationDirectory: string, expectedKind: RemoteFileKind) => required<void>('move_remote_entry', { sessionId, generation, path, destinationDirectory, expectedKind }),
  extractRemoteArchive: (sessionId: string, generation: number, path: string, separateDirectory: boolean, overwrite: boolean) => required<string>('extract_remote_archive', { sessionId, generation, path, separateDirectory, overwrite }),
  selectLocalFile: async () => {
    if (!isDesktop) return null;
    const selected = await open({ multiple: false, directory: false });
    return typeof selected === 'string' ? selected : null;
  },
  selectLocalDestination: (defaultPath: string) => isDesktop ? save({ defaultPath }) : Promise.resolve(null),
  startUpload: (sessionId: string, generation: number, localPath: string, remotePath: string, overwrite: boolean) => required<string>('start_upload', { sessionId, generation, localPath, remotePath, overwrite }),
  startDownload: (sessionId: string, generation: number, remotePath: string, localPath: string, overwrite: boolean) => required<string>('start_download', { sessionId, generation, remotePath, localPath, overwrite }),
  cancelTransfer: (transferId: string, sessionId: string, generation: number) => required<void>('cancel_transfer', { transferId, sessionId, generation }),
  aiPropose: (intent: string, locale: 'zh' | 'en', target: TargetRef, input: AiContextData, permissions: AiContextPermissions) => required<{ summary: string; plan: AiPlan }>('ai_propose', { intent, locale: locale === 'zh' ? 'zh-CN' : 'en-US', target, input, permissions }),
  aiAnalyze: (executionId: string, locale: 'zh' | 'en', target: TargetRef, authorizeRecentResults: boolean) => required<{ executionId: string; analysis: string }>('ai_analyze', { executionId, locale: locale === 'zh' ? 'zh-CN' : 'en-US', target, authorizeRecentResults }),
  aiAgentTurn: (intent: string, taskId: string, history: AiConversationMessage[], locale: 'zh' | 'en', target: TargetRef, input: AiContextData, permissions: AiContextPermissions, executionIds: string[], sessionId: string | null) => required<AiAgentTurnResult>('ai_agent_turn', { request: { intent, taskId, history, locale: locale === 'zh' ? 'zh-CN' : 'en-US', target, input, permissions, executionIds, sessionId } }),
  aiEntitlement: () => required<AiEntitlement>('ai_entitlement'),
  aiLicenseKey: () => required<string | null>('ai_license_key'),
  aiCreateOrder: (plan: 'personal' | 'high') => required<AiBillingOrder>('ai_create_order', { plan }),
  aiActivateLicense: (licenseKey: string) => required<AiEntitlement>('ai_activate_license', { licenseKey }),
  aiDeactivateLicense: () => required<AiEntitlement>('ai_deactivate_license'),
  aiReleaseTask: (taskId: string) => required<void>('ai_release_task', { taskId }),
  aiCreatePlan: (target: TargetRef, proposals: CommandProposal[]) => required<AiPlan>('ai_create_plan', { target, proposals }),
  aiRunNext: (planId: string, target: TargetRef, mode: ExecutionMode) => required<StepProgress>('ai_run_next', { planId, target, mode }),
  aiApproveAndRun: (planId: string, approvalId: string, command: string, target: TargetRef) => required<StepProgress>('ai_approve_and_run', { planId, approvalId, command, target }),
  aiStop: (planId: string) => required<AiPlan>('ai_stop', { planId }),
  aiDisconnected: (target: TargetRef) => required<void>('ai_disconnected', { target }),
  batchCreate: (spec: BatchSpec) => required<string>('batch_create', { spec }),
  batchSnapshot: (runId: string) => required<BatchRun>('batch_snapshot', { runId }),
  batchRun: (runId: string) => required<BatchRun>('batch_run', { runId }),
  batchStop: (runId: string) => required<void>('batch_stop', { runId }),
  workflowCreate: (version: WorkflowVersion) => required<string>('workflow_create', { version }),
  workflowRun: (runId: string, mode: ExecutionMode) => required<WorkflowExecution>('workflow_run', { runId, mode }),
  workflowApproveAndRun: (runId: string, approvalId: string, command: string, mode: ExecutionMode) => required<WorkflowExecution>('workflow_approve_and_run', { runId, approvalId, command, mode }),
  workflowStop: (runId: string) => required<void>('workflow_stop', { runId }),
  automationDefinitionSave: (kind: AutomationDefinitionKind, id: string | null, name: string, definition: Record<string, unknown>) => required<AutomationDefinition>('automation_definition_save', { kind, id, name, definition }),
  automationDefinitions: (kind: AutomationDefinitionKind) => required<AutomationDefinition[]>('automation_definitions', { kind }),
  automationDefinitionDelete: (kind: AutomationDefinitionKind, id: string) => required<void>('automation_definition_delete', { kind, id }),
  automationRunHistory: (limit = 50) => required<AutomationRunRecord[]>('automation_run_history', { limit }),
  onTerminal: (handler: (event: TerminalEvent) => void) => isDesktop ? listen<TerminalEvent>('terminal-output', ({ payload }) => handler(payload)) : Promise.resolve(() => {}),
  onSession: (handler: (event: SessionSnapshot) => void) => isDesktop ? listen<SessionSnapshot>('session-state', ({ payload }) => handler(payload)) : Promise.resolve(() => {}),
  onTransfer: (handler: (event: TransferEvent) => void) => isDesktop ? listen<TransferEvent>('transfer-progress', ({ payload }) => handler(payload)) : Promise.resolve(() => {}),
  onSync: (handler: () => void) => isDesktop ? listen<SyncReport>('sync-completed', () => handler()) : Promise.resolve(() => {}),
  onSyncError: (handler: (reason: string) => void) => isDesktop ? listen<string>('sync-failed', ({ payload }) => handler(payload)) : Promise.resolve(() => {}),
};
