export type ConnectionState =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "disconnected"
  | "failed"
  | "closed";

export type Protocol = "ssh" | "local" | "telnet" | "serial" | "sftp";

export interface SshSpec {
  host: string;
  port: number;
  username?: string | null;
  identityFile?: string | null;
  proxyJump?: string | null;
  name?: string | null;
  term?: string | null;
  connectTimeout?: number | null;
  serverAliveInterval?: number | null;
  x11Forwarding?: boolean;
  forwardAgent?: boolean;
  compression?: boolean;
  certificateFile?: string | null;
  pkcs11Provider?: string | null;
  engine?: string | null;
}

/** A live terminal tab. `id` is the primary backend runtime session id. */
export interface SessionTab {
  id: string;
  title: string;
  protocol: Protocol;
  host?: string;
  state: ConnectionState;
  detail?: string;
  /** Saved profile this tab originated from, if any. */
  savedId?: string | null;
  logging?: boolean;
  logPath?: string | null;
  /** Split panes: session ids, `panes[0]` is the primary. */
  panes: string[];
  paneTitles: Record<string, string>;
  orientation: "row" | "column";
  sizes?: number[];
  activePane: string;
  /** For ad-hoc SSH tabs: the spec used, so it can be saved as a device. */
  sshSpec?: SshSpec | null;
  /** Resolved Backspace behaviour for this tab (`del` or `bs`). */
  backspace?: "del" | "bs";
  /** Output text encoding (`utf-8`, `gbk`, `latin1`, ...). */
  encoding?: string;
}

/** Per-device overrides stored in `SavedSession.settingsOverride`. */
export interface DeviceOverrides {
  backspace?: "del" | "bs";
  engine?: string;
  encoding?: string;
  autolog?: boolean;
  pager?: "none" | "huawei" | "cisco" | "juniper" | "mikrotik" | "custom";
  pagerCommand?: string;
  loginScript?: string;
  vendor?: string;
  serial?: Record<string, unknown>;
}

export interface TerminalOutputEvent {
  sessionId: string;
  seq: number;
  data: string;
}

export interface ConnectionStateEvent {
  sessionId: string;
  state: ConnectionState;
  message?: string | null;
}

export interface TerminalExitEvent {
  sessionId: string;
  reason: string;
}

/** A saved connection profile (matches Rust `SavedSession`). */
export interface SavedSession {
  id: string;
  folderId: string | null;
  name: string;
  host: string;
  port: number;
  protocol: Protocol;
  username: string | null;
  sshAlias: string | null;
  identityFile: string | null;
  proxyJump: string | null;
  tags: string[];
  description: string | null;
  notes: string | null;
  settingsOverride: Record<string, unknown> | null;
  credentialRef: string | null;
  isFavorite: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface SavedSessionInput {
  id?: string | null;
  folderId?: string | null;
  name: string;
  host: string;
  port: number;
  protocol: Protocol;
  username?: string | null;
  sshAlias?: string | null;
  identityFile?: string | null;
  proxyJump?: string | null;
  tags?: string[];
  description?: string | null;
  notes?: string | null;
  settingsOverride?: Record<string, unknown> | null;
  credentialRef?: string | null;
  isFavorite?: boolean;
  sortOrder?: number | null;
}

export interface Folder {
  id: string;
  parentId: string | null;
  name: string;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface FolderInput {
  id?: string | null;
  parentId?: string | null;
  name: string;
  sortOrder?: number | null;
}

export interface TerminalSettings {
  fontFamily: string;
  fontSize: number;
  scrollback: number;
  colorScheme: string;
  cursorStyle: string;
  cursorBlink: boolean;
  /** What Backspace sends: `del` (0x7f) or `bs` (0x08, Huawei/Cisco). */
  backspace: string;
  copyOnSelect: boolean;
}

export interface GeneralSettings {
  confirmClose: boolean;
  defaultProtocol: string;
  reopenLastTabs: boolean;
  autoReconnect: boolean;
}

export interface SshSettings {
  connectTimeoutSecs: number;
  serverAliveIntervalSecs: number;
  term: string;
  identityFile: string | null;
  compression: boolean;
  forwardAgent: boolean;
  x11Forwarding: boolean;
  certificateFile: string | null;
  pkcs11Provider: string | null;
  engine: string;
}

export interface LoggingSettings {
  enabledByDefault: boolean;
  logDir: string | null;
  mode: string;
  timestamps: boolean;
}

export interface AppearanceSettings {
  uiTheme: string;
  density: string;
}

export interface AppSettings {
  general: GeneralSettings;
  terminal: TerminalSettings;
  ssh: SshSettings;
  logging: LoggingSettings;
  appearance: AppearanceSettings;
}

export interface Command {
  id: string;
  groupId: string | null;
  name: string;
  content: string;
  vendorProfile: string | null;
  executeMode: string;
  riskLevel: string;
  variables: Record<string, unknown>;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface CommandInput {
  id?: string | null;
  groupId?: string | null;
  name: string;
  content: string;
  vendorProfile?: string | null;
  executeMode?: string;
  riskLevel?: string;
  variables?: Record<string, unknown>;
  tags?: string[];
}

export interface CommandGroup {
  id: string;
  parentId: string | null;
  name: string;
  sortOrder: number;
}

export interface Runbook {
  id: string;
  name: string;
  description: string | null;
  script: string;
  createdAt: string;
  updatedAt: string;
}

export interface Workspace {
  id: string;
  name: string;
  description: string | null;
  layout: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
}

export interface LogEntry {
  id: string;
  sessionId: string | null;
  label: string | null;
  filePath: string;
  startedAt: string;
  endedAt: string | null;
  sizeBytes: number;
}

export interface RecentConnection {
  id: string;
  sessionId: string | null;
  safeHostLabel: string;
  lastUsedAt: string;
}

export interface LogStartResult {
  id: string;
  path: string;
}

export interface CredentialRecord {
  name: string;
  username: string | null;
  kind: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface DispatchResult {
  sessionId: string;
  ok: boolean;
  error: string | null;
}

export interface WaitResult {
  matched: boolean;
  text: string;
}

export interface TunnelSpec {
  kind: "local" | "remote" | "dynamic";
  bindAddress: string;
  bindPort: number;
  destHost?: string | null;
  destPort?: number | null;
  sshHost: string;
  sshPort: number;
  sshUser?: string | null;
  identityFile?: string | null;
  label?: string | null;
}

export interface TunnelView {
  id: string;
  kind: string;
  bind: string;
  dest: string | null;
  sshHost: string;
  label: string | null;
  running: boolean;
  error: string | null;
}

export interface SftpEntry {
  name: string;
  path: string;
  isDir: boolean;
  isLink: boolean;
  size: number;
  permissions: string;
  modified: string;
}

export interface TransferRequest {
  spec: SshSpec;
  direction: "upload" | "download";
  localPath: string;
  remotePath: string;
  recursive?: boolean;
}

export interface TransferView {
  id: string;
  direction: string;
  localPath: string;
  remotePath: string;
  status: string;
  message: string | null;
}

export interface AutomationStep {
  type: "send" | "waitFor" | "sleep";
  sessionId?: string;
  data?: string;
  appendNewline?: boolean;
  pattern?: string;
  timeoutMs?: number;
  ms?: number;
}

export interface AutomationStepResult {
  index: number;
  kind: string;
  ok: boolean;
  detail: string;
}
