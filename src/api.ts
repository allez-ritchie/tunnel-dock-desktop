import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AuthType = "password" | "private_key";
export type TunnelPhase =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

export type AppBlockReason =
  | "none"
  | "tunnel_deleted"
  | "group_missing"
  | "tunnel_down"
  | "forward_failed"
  | "port_conflict";

export type TestState = "success" | "failure";

export interface CredentialState {
  authType: AuthType;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
}

export interface JumpView {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  credentials: CredentialState;
  /**
   * 凭据由哪个隧道提供：跳板机与某条已配置的隧道是同一台主机时，密码无需重复输入。
   * null 表示使用跳板机自己的凭据。
   */
  inheritedFrom: string | null;
}

/**
 * 一次确定的 SSH 接入：用户名 / 地址 / 端口 / 跳板链。
 *
 * 隧道只描述一条接入路径，不持有本地端口与远程目标；需要多个入口时由应用持有
 * 多条隧道并按顺序尝试，本地端口对第三方保持不变。
 */
export interface TunnelView {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  credentials: CredentialState;
  jumps: JumpView[];
  autoReconnect: boolean;
  keepAliveSeconds: number;
  connectTimeoutSeconds: number;
  strictHostKey: boolean;
  createdAt: number;
  updatedAt: number;
  /** `user@host:port`，列表里直接展示。 */
  endpoint: string;
  chainSummary: string;
  /** 地址 / 用户名 / 端口 / 凭据齐全。 */
  dialable: boolean;
}

/** One `localPort -> remoteHost:remotePort` forwarding rule. */
export interface AppView {
  id: string;
  name: string;
  groupId: string;
  /** 按尝试顺序排列的隧道，第一条是首选。 */
  tunnelIds: string[];
  localPort: number;
  remoteHost: string;
  remotePort: number;
  browserUrl: string;
  browserOpenUrl: string;
  localUrl: string;
  autoStartOnBoot: boolean;
  desiredOn: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface GroupView {
  id: string;
  name: string;
  isDefault: boolean;
  createdAt: number;
}

/** Why one tunnel could not be used during the most recent connect round. */
export interface TunnelFailure {
  tunnelId: string;
  name: string;
  /** 语言无关的错误码，文案由界面提供。 */
  code: string;
  detail: string | null;
}

/** Which tunnel is actually carrying the traffic right now. */
export interface TunnelSelection {
  tunnelId: string;
  name: string;
  endpoint: string;
}

export interface AppStatus {
  appId: string;
  phase: TunnelPhase;
  blockReason: AppBlockReason;
  attempt: number;
  nextRetrySeconds: number | null;
  connectedAt: number | null;
  localPort: number | null;
  errorCode: string | null;
  errorDetail: string | null;
  securityAlert: boolean;
  /** 当前（或最后一次尝试）承载转发的隧道。 */
  tunnel: TunnelSelection | null;
  /** 本轮已经失败的隧道，按尝试顺序。 */
  tunnelFailures: TunnelFailure[];
  updatedAt: number;
}

export interface TunnelTestResult {
  tunnelId: string;
  state: TestState;
  elapsedMs: number;
  errorCode: string | null;
  errorDetail: string | null;
  securityAlert: boolean;
}

export interface DockView {
  tunnels: TunnelView[];
  apps: AppView[];
  groups: GroupView[];
  statuses: AppStatus[];
}

export interface CredentialInput {
  authType: AuthType;
  /** null keeps the stored secret, "" clears it, any other value replaces it. */
  password: string | null;
  privateKey: string | null;
  passphrase: string | null;
  /**
   * `~/.ssh` 下的文件名。设置后由后端读取该文件作为私钥，
   * 前端不再接触密钥内容；与 `privateKey` 同时出现时以本字段为准。
   */
  privateKeyPath?: string | null;
}

export interface JumpInput {
  id: string | null;
  label: string;
  host: string;
  port: number;
  username: string;
  credentials: CredentialInput;
}

export interface TunnelInput {
  id: string | null;
  name: string;
  host: string;
  port: number;
  username: string;
  credentials: CredentialInput;
  jumps: JumpInput[];
  autoReconnect: boolean;
  keepAliveSeconds: number;
  connectTimeoutSeconds: number;
  strictHostKey: boolean;
}

export interface AppInput {
  id: string | null;
  name: string;
  groupId: string;
  /** 按尝试顺序排列的隧道，第一条是首选。 */
  tunnelIds: string[];
  localPort: number;
  remoteHost: string;
  remotePort: number;
  browserUrl: string;
  autoStartOnBoot: boolean;
}

export interface GroupInput {
  id: string | null;
  name: string;
}

/** `~/.ssh` 下的一个私钥文件。 */
export interface LocalSshKey {
  name: string;
  path: string;
  /** `ssh-rsa` / `ssh-ed25519` 等；无法判断时为空串。 */
  keyType: string;
  encrypted: boolean;
}

export interface LocalSshStatus {
  available: boolean;
  root: string;
  keys: LocalSshKey[];
  configAvailable: boolean;
  configPath: string;
  configHostCount: number;
  configImportableCount: number;
}

/** `~/.ssh/config` 里的一条 Host。 */
export interface SshConfigHost {
  alias: string;
  hostName: string;
  user: string;
  port: number;
  identityFile: string | null;
  proxyJump: string[];
  importable: boolean;
  /** `proxy_command` / `include` / `match` / `proxy_jump`。 */
  unsupported: string | null;
}

export interface ExportSummary {
  path: string;
  tunnels: number;
  groups: number;
  apps: number;
  /** 使用密码认证、因此没有导出密码的隧道。 */
  passwordTunnels: string[];
  /** 使用密码认证、因此没有导出凭据的跳板机。 */
  passwordJumps: string[];
}

export interface BundlePreview {
  tunnelCount: number;
  groupCount: number;
  appCount: number;
  tunnelNames: string[];
  groupNames: string[];
  exportedAt: number;
  /** 清单里缺少私钥的隧道数量：导入后需要手动补凭据。 */
  tunnelsWithoutKey: number;
}

export interface ImportSummary {
  tunnelsAdded: number;
  tunnelsReused: number;
  groupsAdded: number;
  groupsReused: number;
  appsAdded: number;
  appsSkipped: number;
  /** 与现有应用撞了本地端口，导入后需要改端口才能启动。 */
  portConflicts: string[];
  appsDropped: number;
}

export const api = {
  loadDock: () => invoke<DockView>("load_dock"),
  saveTunnel: (input: TunnelInput) => invoke<TunnelView>("save_tunnel", { input }),
  deleteTunnel: (id: string) => invoke<void>("delete_tunnel", { id }),
  testTunnel: (id: string) => invoke<TunnelTestResult>("test_tunnel", { id }),
  saveApp: (input: AppInput) => invoke<AppView>("save_app", { input }),
  deleteApp: (id: string) => invoke<void>("delete_app", { id }),
  startApp: (id: string) => invoke<void>("start_app", { id }),
  stopApp: (id: string) => invoke<void>("stop_app", { id }),
  restartApp: (id: string) => invoke<void>("restart_app", { id }),
  startGroup: (groupId: string) => invoke<void>("start_group", { groupId }),
  stopGroup: (groupId: string) => invoke<void>("stop_group", { groupId }),
  saveGroup: (input: GroupInput) => invoke<GroupView>("save_group", { input }),
  deleteGroup: (id: string) => invoke<void>("delete_group", { id }),
  localSshStatus: () => invoke<LocalSshStatus>("local_ssh_status"),
  localSshConfigHosts: () =>
    invoke<SshConfigHost[]>("local_ssh_config_hosts"),
  /** 把选中的 `~/.ssh/config` 别名导入为隧道；已存在的别名会被跳过。 */
  importLocalSshConfig: (aliases: string[]) =>
    invoke<TunnelView[]>("import_local_ssh_config", { aliases }),
  exportBundle: (path: string) => invoke<ExportSummary>("export_bundle", { path }),
  previewBundle: (path: string) => invoke<BundlePreview>("preview_bundle", { path }),
  importBundle: (path: string) => invoke<ImportSummary>("import_bundle", { path }),
  knownHostsCount: () => invoke<number>("known_hosts_count"),
  clearKnownHosts: () => invoke<void>("clear_known_hosts"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  appVersion: () => invoke<string>("app_version"),
};

export function onAppStatus(
  callback: (status: AppStatus) => void,
): Promise<UnlistenFn> {
  return listen<AppStatus>("app-status", (event) => callback(event.payload));
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
