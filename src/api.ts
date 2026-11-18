import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AuthType = "password" | "private_key";
export type TunnelPhase =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error"
  | "stopping";

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

export interface TunnelView {
  id: string;
  name: string;
  localHost: string;
  localPort: number;
  remoteHost: string;
  remotePort: number;
  sshHost: string;
  sshPort: number;
  sshUser: string;
  credentials: CredentialState;
  jumps: JumpView[];
  autoReconnect: boolean;
  autoStartOnBoot: boolean;
  keepAliveSeconds: number;
  connectTimeoutSeconds: number;
  strictHostKey: boolean;
  browserUrl: string;
  desiredOn: boolean;
  createdAt: number;
  updatedAt: number;
  localUrl: string;
  browserOpenUrl: string;
  chainSummary: string;
}

export interface TunnelStatus {
  tunnelId: string;
  phase: TunnelPhase;
  attempt: number;
  nextRetrySeconds: number | null;
  connectedAt: number | null;
  localPort: number | null;
  errorCode: string | null;
  errorDetail: string | null;
  securityAlert: boolean;
  updatedAt: number;
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
  localHost: string;
  localPort: number;
  remoteHost: string;
  remotePort: number;
  sshHost: string;
  sshPort: number;
  sshUser: string;
  credentials: CredentialInput;
  jumps: JumpInput[];
  autoReconnect: boolean;
  autoStartOnBoot: boolean;
  keepAliveSeconds: number;
  connectTimeoutSeconds: number;
  strictHostKey: boolean;
  browserUrl: string;
}

export const api = {
  listTunnels: () => invoke<TunnelView[]>("list_tunnels"),
  listStatuses: () => invoke<TunnelStatus[]>("list_statuses"),
  saveTunnel: (input: TunnelInput) => invoke<TunnelView>("save_tunnel", { input }),
  deleteTunnel: (id: string) => invoke<void>("delete_tunnel", { id }),
  connectTunnel: (id: string) => invoke<void>("connect_tunnel", { id }),
  connectAll: () => invoke<void>("connect_all"),
  disconnectTunnel: (id: string) => invoke<void>("disconnect_tunnel", { id }),
  disconnectAll: () => invoke<void>("disconnect_all"),
  reconnectTunnel: (id: string) => invoke<void>("reconnect_tunnel", { id }),
  knownHostsCount: () => invoke<number>("known_hosts_count"),
  clearKnownHosts: () => invoke<void>("clear_known_hosts"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  appVersion: () => invoke<string>("app_version"),
};

export function onTunnelStatus(
  callback: (status: TunnelStatus) => void,
): Promise<UnlistenFn> {
  return listen<TunnelStatus>("tunnel-status", (event) => callback(event.payload));
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
