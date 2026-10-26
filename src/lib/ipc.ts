import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AutomationStep,
  AutomationStepResult,
  Command,
  CommandGroup,
  CommandInput,
  CredentialRecord,
  DispatchResult,
  Folder,
  FolderInput,
  LogEntry,
  LogStartResult,
  RecentConnection,
  Runbook,
  SavedSession,
  SavedSessionInput,
  SftpEntry,
  SshSpec,
  TransferRequest,
  TransferView,
  TunnelSpec,
  TunnelView,
  WaitResult,
  Workspace,
} from "../types";

/** Startup capability check for the OpenSSH client. */
export function sshAvailable(): Promise<boolean> {
  return invoke<boolean>("ssh_available");
}

// ---- Connections ---------------------------------------------------------

export function openLocal(cols: number, rows: number): Promise<string> {
  return invoke<string>("connection_open_local", { cols, rows });
}

export function openSsh(
  spec: SshSpec,
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("connection_open_ssh", { spec, cols, rows });
}

export function openSaved(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("connection_open_saved", { sessionId, cols, rows });
}

export function openTelnet(
  spec: {
    host: string;
    port: number;
    username?: string | null;
    term?: string | null;
  },
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("connection_open_telnet", { spec, cols, rows });
}

export function openSerial(
  spec: {
    device: string;
    baud: number;
    dataBits: number;
    parity: string;
    stopBits: number;
    flowControl: string;
    newline: string;
  },
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("connection_open_serial", { spec, cols, rows });
}

export function serialDevicesList(): Promise<string[]> {
  return invoke<string[]>("serial_devices_list");
}

export function serialBreak(sessionId: string): Promise<void> {
  return invoke("connection_serial_break", { sessionId });
}

export function connectionWrite(
  sessionId: string,
  data: string,
): Promise<void> {
  return invoke("connection_write", { sessionId, data });
}

export function connectionResize(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("connection_resize", { sessionId, cols, rows });
}

export function connectionClose(sessionId: string): Promise<void> {
  return invoke("connection_close", { sessionId });
}

export function connectionReconnect(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("connection_reconnect", { sessionId, cols, rows });
}

// ---- Saved sessions & folders -------------------------------------------

export function sessionsList(): Promise<SavedSession[]> {
  return invoke<SavedSession[]>("sessions_list");
}

export function sessionGet(id: string): Promise<SavedSession> {
  return invoke<SavedSession>("session_get", { id });
}

export function sessionSave(input: SavedSessionInput): Promise<SavedSession> {
  return invoke<SavedSession>("session_save", { input });
}

export function sessionDelete(id: string): Promise<void> {
  return invoke("session_delete", { id });
}

export function foldersList(): Promise<Folder[]> {
  return invoke<Folder[]>("folders_list");
}

export function folderSave(input: FolderInput): Promise<Folder> {
  return invoke<Folder>("folder_save", { input });
}

export function folderDelete(id: string): Promise<void> {
  return invoke("folder_delete", { id });
}

// ---- Commands ------------------------------------------------------------

export function commandGroupsList(): Promise<CommandGroup[]> {
  return invoke<CommandGroup[]>("command_groups_list");
}

export function commandGroupSave(input: {
  id?: string | null;
  parentId?: string | null;
  name: string;
  sortOrder?: number | null;
}): Promise<CommandGroup> {
  return invoke<CommandGroup>("command_group_save", { input });
}

export function commandGroupDelete(id: string): Promise<void> {
  return invoke("command_group_delete", { id });
}

export function commandsList(): Promise<Command[]> {
  return invoke<Command[]>("commands_list");
}

export function commandSave(input: CommandInput): Promise<Command> {
  return invoke<Command>("command_save", { input });
}

export function commandDelete(id: string): Promise<void> {
  return invoke("command_delete", { id });
}

export function commandDispatch(
  sessionIds: string[],
  content: string,
  appendEnter: boolean,
): Promise<DispatchResult[]> {
  return invoke<DispatchResult[]>("command_dispatch", {
    sessionIds,
    content,
    appendEnter,
  });
}

// ---- Runbooks ------------------------------------------------------------

export function runbooksList(): Promise<Runbook[]> {
  return invoke<Runbook[]>("runbooks_list");
}

export function runbookSave(input: {
  id?: string | null;
  name: string;
  description?: string | null;
  script?: string;
}): Promise<Runbook> {
  return invoke<Runbook>("runbook_save", { input });
}

export function runbookDelete(id: string): Promise<void> {
  return invoke("runbook_delete", { id });
}

// ---- Workspaces ----------------------------------------------------------

export function workspacesList(): Promise<Workspace[]> {
  return invoke<Workspace[]>("workspaces_list");
}

export function workspaceSave(input: {
  id?: string | null;
  name: string;
  description?: string | null;
  layout?: Record<string, unknown>;
}): Promise<Workspace> {
  return invoke<Workspace>("workspace_save", { input });
}

export function workspaceDelete(id: string): Promise<void> {
  return invoke("workspace_delete", { id });
}

// ---- Settings ------------------------------------------------------------

export function settingsGet(): Promise<AppSettings> {
  return invoke<AppSettings>("settings_get");
}

export function settingsSet(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("settings_set", { settings });
}

// ---- Logging -------------------------------------------------------------

export function logStart(
  sessionId: string,
  label: string | null,
  mode: string,
  timestamps: boolean,
): Promise<LogStartResult> {
  return invoke<LogStartResult>("log_start", {
    sessionId,
    label,
    mode,
    timestamps,
  });
}

export function logStop(sessionId: string): Promise<void> {
  return invoke("log_stop", { sessionId });
}

export function logStatus(sessionId: string): Promise<boolean> {
  return invoke<boolean>("log_status", { sessionId });
}

export function logsList(): Promise<LogEntry[]> {
  return invoke<LogEntry[]>("logs_list");
}

export function logRead(path: string): Promise<string> {
  return invoke<string>("log_read", { path });
}

export function recentList(): Promise<RecentConnection[]> {
  return invoke<RecentConnection[]>("recent_list");
}

// ---- SSH tunnels ---------------------------------------------------------

export function tunnelStart(spec: TunnelSpec): Promise<string> {
  return invoke<string>("tunnel_start", { spec });
}

export function tunnelList(): Promise<TunnelView[]> {
  return invoke<TunnelView[]>("tunnel_list");
}

export function tunnelStop(id: string): Promise<void> {
  return invoke("tunnel_stop", { id });
}

// ---- File transfer -------------------------------------------------------

export function transferStart(
  request: TransferRequest,
): Promise<string> {
  return invoke<string>("sftp_transfer_start", { request });
}

export function transfersList(): Promise<TransferView[]> {
  return invoke<TransferView[]>("transfers_list");
}

export function transferCancel(id: string): Promise<void> {
  return invoke("transfer_cancel", { id });
}

export function sftpList(sessionId: string, path: string): Promise<SftpEntry[]> {
  return invoke<SftpEntry[]>("sftp_list", { sessionId, path });
}

export function sftpMkdir(sessionId: string, path: string): Promise<void> {
  return invoke("sftp_mkdir", { sessionId, path });
}

export function sftpRemove(
  sessionId: string,
  path: string,
  isDir: boolean,
): Promise<void> {
  return invoke("sftp_remove", { sessionId, path, isDir });
}

export function sftpRename(
  sessionId: string,
  from: string,
  to: string,
): Promise<void> {
  return invoke("sftp_rename", { sessionId, from, to });
}

export function openSftp(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("connection_open_sftp", { sessionId, cols, rows });
}

// ---- Automation ----------------------------------------------------------

export function sessionWaitFor(
  sessionId: string,
  pattern: string,
  timeoutMs: number,
): Promise<WaitResult> {
  return invoke<WaitResult>("session_wait_for", {
    sessionId,
    pattern,
    timeoutMs,
  });
}

export function automationRun(
  steps: AutomationStep[],
): Promise<AutomationStepResult[]> {
  return invoke<AutomationStepResult[]>("automation_run", { steps });
}

// ---- Credentials ---------------------------------------------------------

export function credentialSet(
  name: string,
  secret: string,
  username: string | null,
  kind: string,
  tags: string[],
): Promise<void> {
  return invoke("credential_set", { name, secret, username, kind, tags });
}

export function credentialGet(name: string): Promise<string> {
  return invoke<string>("credential_get", { name });
}

export function credentialDelete(name: string): Promise<void> {
  return invoke("credential_delete", { name });
}

export function credentialsList(): Promise<CredentialRecord[]> {
  return invoke<CredentialRecord[]>("credentials_list");
}
