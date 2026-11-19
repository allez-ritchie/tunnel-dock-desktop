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
}

/** Reusable SSH access definition. Holds no local port and no remote target. */
export interface TunnelView {
  id: string;
  name: string;
  sshHost: string;
  sshPort: number;
  sshUser: string;
  credentials: CredentialState;
  jumps: JumpView[];
  autoReconnect: boolean;
  keepAliveSeconds: number;
  connectTimeoutSeconds: number;
  strictHostKey: boolean;
  createdAt: number;
  updatedAt: number;
  endpoint: string;
  chainSummary: string;
}

/** One `localPort -> remoteHost:remotePort` forwarding rule. */
export interface AppView {
  id: string;
  name: string;
  groupId: string;
  tunnelId: string;
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
  sshHost: string;
  sshPort: number;
  sshUser: string;
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
  tunnelId: string;
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
