export type AuthType = 'password' | 'private_key';
export type ConnectionState = 'disconnected' | 'resolving' | 'verifying_target' | 'authenticating' | 'connected' | 'host_key_blocked' | 'error';

export interface Folder {
  id: string;
  parentId: string | null;
  name: string;
  sortOrder: number;
  revision: number;
  updatedAt: string;
}

export interface ServerInput {
  id: string | null;
  folderId: string | null;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  privateKeyPathHint: string | null;
  jumpServerId: string | null;
}

export interface ServerRecord extends Omit<ServerInput, 'id'> {
  id: string;
  sortOrder: number;
  revision: number;
  updatedAt: string;
  deletedAt: string | null;
}

export interface CatalogSnapshot {
  folders: Folder[];
  servers: ServerRecord[];
}

export interface CatalogOrderItem {
  id: string;
  parentId: string | null;
  sortOrder: number;
}

export interface TargetRef {
  serverId: string;
  sessionId: string;
  generation: number;
}

export interface SessionSnapshot {
  target: TargetRef;
  state: ConnectionState;
  channelId: string | null;
  errorCode: string | null;
}

export interface HostKeyChallenge {
  host: string;
  port: number;
  algorithm: string;
  publicKey: string;
  fingerprint: string;
  previousFingerprint: string | null;
}

export interface ConnectResult {
  session: SessionSnapshot | null;
  hostKeyChallenge: HostKeyChallenge | null;
}

export interface TestConnectionResult {
  hostKeyChallenge: HostKeyChallenge | null;
}

export interface TerminalEvent {
  target: TargetRef;
  channelId: string;
  data: number[];
}

export interface Preferences {
  theme: 'dark' | 'light';
  locale: 'zh' | 'en';
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalCompletionEnabled: boolean;
  aiMaxSteps: number;
}

export interface AccountSession {
  userId: string;
  deviceId: string;
  expiresAt: string;
  email: string | null;
  nickname: string | null;
}

export interface AccountAuthResult {
  session: AccountSession;
  requiresChoice: boolean;
  previousUserId: string | null;
  localFolders: number;
  localServers: number;
}

export type AccountSwitchChoice = 'cloud_replace_local' | 'local_replace_cloud' | 'merge';

export interface AccountSwitchResult {
  applied: boolean;
  syncReport: SyncReport | null;
  syncError: string | null;
  outbox: number;
  conflicts: number;
}

export type AccountStatus =
  | { state: 'signed_out' }
  | { state: 'signed_in' | 'expired'; session: AccountSession };

export interface UpdateCheck {
  status: 'no_update' | 'update_available';
  version: string | null;
  minimumMacOs: string | null;
  target: string | null;
  channel: string | null;
  updatePolicy: 'force' | 'optional' | 'silent' | null;
  releaseNotes: string | null;
  artifactUrl: string | null;
  artifactBytes: number | null;
  sha256: string | null;
  signature: string | null;
  publishedAt: string | null;
}

export interface FeedbackReceipt {
  feedbackId: string;
  status: 'pending';
}

export interface UpdateDownloadProgress {
  downloaded: number;
  total: number | null;
}

export interface SyncReport {
  pushed: number;
  conflicts: number;
  pulled: number;
  snapshotRestored: boolean;
}

export interface SyncStatus {
  cursor: number;
  outbox: number;
  conflicts: number;
}

export interface SyncConflict {
  conflictId: string;
  entityType: 'folder' | 'server' | 'batch_template' | 'workflow_definition';
  entityId: string;
  localPayload: Record<string, unknown>;
  remotePayload: { revision?: number; deleted?: boolean; data?: Record<string, unknown> };
  remoteRevision: number;
  detectedAt: string;
}

export interface AiContextData {
  serverLabel?: string;
  systemInfo?: string;
  workingDirectory?: string;
  selectedOutput?: string;
  recentResults?: string[];
  directoryEntries?: string[];
}

export interface AiContextPermissions {
  serverLabel: boolean;
  systemInfo: boolean;
  workingDirectory: boolean;
  selectedOutput: boolean;
  recentResults: boolean;
  directoryEntries: boolean;
}

export interface DiskUsage {
  filesystem: string;
  mountPoint: string;
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
}

export interface ProcessInfo {
  pid: number;
  ppid: number;
  user: string;
  cpuPercent: number;
  memoryPercent: number;
  startId: string;
  command: string;
}

export interface SystemSnapshot {
  target: TargetRef;
  collectedAt: string;
  hostname: string;
  osName: string;
  kernel: string;
  architecture: string;
  cpuCores?: number;
  uptimeSeconds: number;
  loadAverage: [number, number, number];
  cpuUsagePercent: number;
  memoryTotalBytes: number;
  memoryUsedBytes: number;
  disks: DiskUsage[];
  processes: ProcessInfo[];
}

export type RemoteFileKind = 'directory' | 'file' | 'symlink' | 'other';
export type ProcessSignal = 'terminate' | 'kill';

export interface RemoteEntry {
  name: string;
  path: string;
  kind: RemoteFileKind;
  size: number | null;
  modifiedAt: number | null;
  permissions: number | null;
}

export interface RemoteDirectory {
  target: TargetRef;
  path: string;
  entries: RemoteEntry[];
  truncated: boolean;
}

export type TransferDirection = 'upload' | 'download';
export type TransferState = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface TransferEvent {
  transferId: string;
  target: TargetRef;
  direction: TransferDirection;
  source: string;
  destination: string;
  bytesTotal: number;
  bytesDone: number;
  bytesPerSecond: number;
  state: TransferState;
  errorCode: 'already_exists' | 'cancelled' | 'invalid_path' | 'operation_failed' | null;
}

export interface CommandHistoryEntry {
  id: string;
  serverId: string;
  command: string;
  createdAt: string;
}

export type ExecutionMode = 'confirm_each' | 'auto_except_high_risk' | 'auto_all';
export type RiskLevel = 'low' | 'unknown' | 'high';
export type AiPlanState = 'ready' | 'running' | 'awaiting_approval' | 'stopping' | 'paused_disconnected' | 'stopped' | 'completed' | 'failed' | 'result_unknown';
export type AiStepState = 'queued' | 'awaiting_approval' | 'running' | 'stop_requested' | 'paused_disconnected' | 'succeeded' | 'failed' | 'stopped' | 'unknown' | 'unknown_after_disconnect';
export type ExecutionState = 'awaiting_approval' | 'authorized' | 'running' | 'stop_requested' | 'succeeded' | 'failed' | 'stopped' | 'paused_disconnected' | 'unknown' | 'unknown_after_disconnect' | 'expired';

export interface CommandProposal {
  command: string;
  modelRisk: RiskLevel | null;
}

export interface AiStep {
  proposal: CommandProposal;
  state: AiStepState;
  executionId: string | null;
  approvalId: string | null;
}

export interface AiPlan {
  planId: string;
  target: TargetRef;
  state: AiPlanState;
  steps: AiStep[];
  currentStep: number | null;
}

export interface RiskAssessment {
  level: RiskLevel;
  reasons: string[];
}

export interface PendingApproval {
  approvalId: string;
  executionId: string;
  target: TargetRef;
  command: string;
  commandHash: string;
  risk: RiskAssessment;
  expiresAt: string;
}

export interface ExecutionRecord {
  executionId: string;
  source: 'ai' | 'batch' | 'workflow';
  target: TargetRef;
  mode: ExecutionMode;
  command: string;
  commandHash: string;
  risk: RiskAssessment;
  state: ExecutionState;
  approvalId: string | null;
  approvalExpiresAt: string | null;
  stdout: string;
  stderr: string;
  exitStatus: number | null;
  errorCode: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface StepProgress {
  plan: AiPlan;
  approval: PendingApproval | null;
  execution: ExecutionRecord | null;
}

export interface AiConversationMessage {
  role: 'user' | 'assistant';
  content: string;
}

export type AiAgentTurnResult =
  | { type: 'tool_call'; summary: string; plan: AiPlan; sessionId: string }
  | { type: 'final_answer'; answer: string; sessionId: string };

export interface AiEntitlement {
  plan: 'free' | 'personal' | 'high';
  licensed: boolean;
  bound: boolean;
  quota: number;
  used: number;
  remaining: number;
  periodStart: string | null;
  periodEnd: string | null;
  expired: boolean;
  previousPlan: 'personal' | 'high' | null;
  previousPeriodEnd: string | null;
  paymentEnabled: boolean;
  mockPayment: boolean;
  paymentAmountOverride: number | null;
}

export interface AiBillingOrder {
  token: string;
  plan: 'personal' | 'high';
  amount: number;
  status: string;
  checkoutUrl: string;
  mock: boolean;
}

export type BatchState = 'queued' | 'running' | 'succeeded' | 'partially_succeeded' | 'failed' | 'stopped' | 'unknown';
export type TargetRunState = 'queued' | 'running' | 'succeeded' | 'failed' | 'stopped' | 'unknown';
export interface BatchSpec {
  command: string;
  targets: TargetRef[];
  maxConcurrent: number;
  mode: ExecutionMode;
}
export interface TargetRun {
  target: TargetRef;
  state: TargetRunState;
  executionId: string | null;
  exitStatus: number | null;
  errorCode: string | null;
  stdout: string;
  stderr: string;
}
export interface BatchRun {
  id: string;
  spec: BatchSpec;
  state: BatchState;
  targets: TargetRun[];
  createdAt: string;
  finishedAt: string | null;
}
export interface WorkflowCommandStep {
  target: TargetRef;
  command: string;
  source: 'workflow' | 'ai';
  timeoutSeconds: number;
  retries: number;
  retryDelaySeconds: number;
}
export type ConditionExpression =
  | { field: 'exit_code'; comparison: 'eq' | 'not_eq' | 'less_than' | 'less_or_equal' | 'greater_than' | 'greater_or_equal'; value: number }
  | { field: 'stdout_contains' | 'stderr_contains'; value: string }
  | { field: 'execution_state'; value: ExecutionState };
export type WorkflowStepKind =
  | { type: 'command'; config: WorkflowCommandStep }
  | { type: 'wait'; config: { seconds: number } }
  | { type: 'condition'; config: ConditionExpression }
  | { type: 'end' }
  | { type: 'failure' };
export interface WorkflowStep {
  id: string;
  kind: WorkflowStepKind;
  onSuccess: string | null;
  onFailure: string | null;
}
export interface WorkflowVersion {
  definitionId: string;
  version: number;
  startStep: string;
  steps: WorkflowStep[];
  maxTransitions: number;
  createdAt: string;
  contentHash: string;
}
export type WorkflowRunState = 'running' | 'awaiting_approval' | 'succeeded' | 'failed' | 'stopped' | 'timed_out' | 'result_unknown';
export interface WorkflowStepResult {
  stepId: string;
  state: ExecutionState;
  executionId: string | null;
  exitStatus: number | null;
  stdout: string;
  stderr: string;
  errorCode: string | null;
}
export interface WorkflowRun {
  id: string;
  definitionId: string;
  version: number;
  versionHash: string;
  state: WorkflowRunState;
  steps: WorkflowStepResult[];
  startedAt: string;
  finishedAt: string | null;
}
export interface WorkflowExecution {
  run: WorkflowRun;
  pendingApproval: PendingApproval | null;
}
export type AutomationDefinitionKind = 'batch_template' | 'workflow_definition';
export interface AutomationDefinition {
  id: string;
  name: string;
  definition: Record<string, unknown>;
  revision: number;
  updatedAt: string;
  deletedAt: string | null;
}
export interface AutomationRunRecord {
  id: string;
  kind: 'batch' | 'workflow';
  state: string;
  payload: BatchRun | WorkflowRun;
  createdAt: string;
  updatedAt: string;
}
