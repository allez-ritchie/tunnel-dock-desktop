import type {
  AppInput,
  AppView,
  AuthType,
  GroupView,
  JumpInput,
  JumpView,
  TunnelInput,
  TunnelView,
} from "./api";
import { strings } from "./strings";

export interface JumpForm {
  id: string | null;
  label: string;
  host: string;
  port: string;
  username: string;
  authType: AuthType;
  passwordInput: string;
  passwordCleared: boolean;
  privateKeyInput: string;
  privateKeyCleared: boolean;
  /** 选中的 `~/.ssh/<名称>`；保存时由后端读取文件，前端不接触内容。 */
  privateKeyPath: string | null;
  passphraseInput: string;
  passphraseCleared: boolean;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
  /** 凭据来自哪条隧道（`host:port` 与某条隧道相同时自动复用其密码）。 */
  inheritedFrom: string | null;
}

/**
 * 一条隧道的可编辑状态：一次确定的 SSH 接入（用户名 / 地址 / 端口 / 跳板 / 凭据）。
 *
 * “多个入口”不再属于隧道：应用持有有序的隧道列表，本地端口始终不变。
 */
export interface TunnelForm {
  id: string | null;
  name: string;
  host: string;
  port: string;
  username: string;
  authType: AuthType;
  passwordInput: string;
  passwordCleared: boolean;
  privateKeyInput: string;
  privateKeyCleared: boolean;
  privateKeyPath: string | null;
  passphraseInput: string;
  passphraseCleared: boolean;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
  jumps: JumpForm[];
  autoReconnect: boolean;
  strictHostKey: boolean;
  keepAliveSeconds: string;
  connectTimeoutSeconds: string;
}

export interface AppForm {
  id: string | null;
  name: string;
  groupId: string;
  /** 按尝试顺序排列的隧道 id，第一条是首选。 */
  tunnelIds: string[];
  localPort: string;
  remoteHost: string;
  remotePort: string;
  browserUrl: string;
  autoStartOnBoot: boolean;
}

export const MAX_JUMPS = 8;
export const MAX_APP_TUNNELS = 8;

export function nextPort(apps: AppView[]): number {
  const used = new Set(apps.map((app) => app.localPort));
  for (let port = 18080; port <= 65535; port += 1) {
    if (!used.has(port)) return port;
  }
  return 18080;
}

export function jumpFormFrom(jump?: JumpView): JumpForm {
  return {
    id: jump?.id ?? null,
    label: jump?.label ?? "",
    host: jump?.host ?? "",
    port: String(jump?.port ?? 22),
    username: jump?.username ?? "root",
    authType: jump?.credentials.authType ?? "password",
    passwordInput: "",
    passwordCleared: false,
    privateKeyInput: "",
    privateKeyCleared: false,
    privateKeyPath: null,
    passphraseInput: "",
    passphraseCleared: false,
    hasPassword: jump?.credentials.hasPassword ?? false,
    hasPrivateKey: jump?.credentials.hasPrivateKey ?? false,
    hasPassphrase: jump?.credentials.hasPassphrase ?? false,
    inheritedFrom: jump?.inheritedFrom ?? null,
  };
}

export function tunnelFormFrom(tunnel?: TunnelView): TunnelForm {
  return {
    id: tunnel?.id ?? null,
    name: tunnel?.name ?? "",
    host: tunnel?.host ?? "",
    port: String(tunnel?.port ?? 22),
    username: tunnel?.username ?? "root",
    authType: tunnel?.credentials.authType ?? "password",
    passwordInput: "",
    passwordCleared: false,
    privateKeyInput: "",
    privateKeyCleared: false,
    privateKeyPath: null,
    passphraseInput: "",
    passphraseCleared: false,
    hasPassword: tunnel?.credentials.hasPassword ?? false,
    hasPrivateKey: tunnel?.credentials.hasPrivateKey ?? false,
    hasPassphrase: tunnel?.credentials.hasPassphrase ?? false,
    jumps: tunnel?.jumps.map((jump) => jumpFormFrom(jump)) ?? [],
    autoReconnect: tunnel?.autoReconnect ?? true,
    strictHostKey: tunnel?.strictHostKey ?? true,
    keepAliveSeconds: String(tunnel?.keepAliveSeconds ?? 30),
    connectTimeoutSeconds: String(tunnel?.connectTimeoutSeconds ?? 15),
  };
}

export function appFormFrom(
  app?: AppView,
  defaults?: { groupId?: string; tunnelIds?: string[]; localPort?: number },
): AppForm {
  return {
    id: app?.id ?? null,
    name: app?.name ?? "",
    groupId: app?.groupId ?? defaults?.groupId ?? "default",
    tunnelIds: app?.tunnelIds ?? defaults?.tunnelIds ?? [],
    localPort: String(app?.localPort ?? defaults?.localPort ?? 18080),
    remoteHost: app?.remoteHost ?? "127.0.0.1",
    remotePort: String(app?.remotePort ?? 80),
    browserUrl: app?.browserUrl ?? "",
    autoStartOnBoot: app?.autoStartOnBoot ?? false,
  };
}

export function signature(form: TunnelForm | AppForm): string {
  return JSON.stringify(form);
}

export function hasSavedPassword(form: JumpForm | TunnelForm): boolean {
  return form.hasPassword && !form.passwordCleared;
}

export function hasSavedPrivateKey(form: JumpForm | TunnelForm): boolean {
  return (form.hasPrivateKey && !form.privateKeyCleared) || usesLocalKey(form);
}

/** 是否选择了 `~/.ssh` 里的私钥文件（保存在本机，不在表单里传内容）。 */
export function usesLocalKey(form: JumpForm | TunnelForm): boolean {
  return (form.privateKeyPath ?? "").trim() !== "";
}

export function hasSavedPassphrase(form: JumpForm | TunnelForm): boolean {
  return form.hasPassphrase && !form.passphraseCleared;
}

export type FormErrors = Record<string, string>;

/** 跳板机的凭据是否复用自另一条隧道（同主机同端口，无需重复输入密码）。 */
export function usesInheritedCredentials(jump: JumpForm): boolean {
  return (jump.inheritedFrom ?? "").trim() !== "";
}

function parsePort(value: string): number | null {
  if (!/^\d{1,5}$/.test(value)) return null;
  const port = Number(value);
  return Number.isInteger(port) ? port : null;
}

export function jumpError(jump: JumpForm): string | null {
  if (jump.host.trim() === "" || /\s/.test(jump.host)) {
    return strings.errorJumpHostRequired;
  }
  const port = parsePort(jump.port);
  if (port === null || port < 1 || port > 65535) {
    return strings.errorJumpPort;
  }
  if (jump.username.trim() === "") {
    return strings.errorJumpUsernameRequired;
  }
  if (jump.authType === "password") {
    if (!hasSavedPassword(jump) && jump.passwordInput === "") {
      return strings.errorJumpPasswordRequired;
    }
  } else if (!hasSavedPrivateKey(jump) && jump.privateKeyInput.trim() === "") {
    return strings.errorJumpPrivateKeyRequired;
  }
  return null;
}

/** 一条隧道自身是否完整，返回第一条错误信息或 `null`。 */
export function tunnelError(tunnel: TunnelForm): string | null {
  if (tunnel.host.trim() === "" || /\s/.test(tunnel.host)) {
    return strings.errorTunnelHostRequired;
  }
  const port = parsePort(tunnel.port);
  if (port === null || port < 1 || port > 65535) {
    return strings.errorTunnelPort;
  }
  if (tunnel.username.trim() === "") {
    return strings.errorTunnelUsernameRequired;
  }
  if (tunnel.authType === "password") {
    if (!hasSavedPassword(tunnel) && tunnel.passwordInput === "") {
      return strings.errorTunnelPasswordRequired;
    }
  } else if (
    !hasSavedPrivateKey(tunnel) &&
    tunnel.privateKeyInput.trim() === ""
  ) {
    return strings.errorTunnelPrivateKeyRequired;
  }
  if (tunnel.jumps.length > MAX_JUMPS) {
    return strings.errorMaxJumps;
  }
  if (tunnel.jumps.some((jump) => jumpError(jump) !== null)) {
    return strings.errorJumpInvalid;
  }
  return null;
}

export function validateTunnel(form: TunnelForm): FormErrors {
  const errors: FormErrors = {};
  const name = form.name.trim();
  if (name === "") errors.name = strings.errorNameRequired;
  else if ([...name].length > 40) errors.name = strings.errorNameTooLong;

  const keepAlive = parsePort(form.keepAliveSeconds);
  if (keepAlive === null || keepAlive < 5 || keepAlive > 300) {
    errors.keepAliveSeconds = strings.errorKeepalive;
  }
  const timeout = parsePort(form.connectTimeoutSeconds);
  if (timeout === null || timeout < 5 || timeout > 120) {
    errors.connectTimeoutSeconds = strings.errorTimeout;
  }

  const tunnelProblem = tunnelError(form);
  if (tunnelProblem !== null) errors.access = tunnelProblem;

  return errors;
}

export function validateApp(
  form: AppForm,
  groups: GroupView[],
  tunnels: TunnelView[],
): FormErrors {
  const errors: FormErrors = {};
  const name = form.name.trim();
  if (name === "") errors.name = strings.errorAppNameRequired;
  else if ([...name].length > 40) errors.name = strings.errorAppNameTooLong;

  if (!groups.some((group) => group.id === form.groupId)) {
    errors.groupId = strings.errorGroupRequired;
  }

  const known = new Set(tunnels.map((tunnel) => tunnel.id));
  const chosen = form.tunnelIds;
  if (chosen.length === 0) {
    errors.tunnelIds = strings.errorTunnelRequired;
  } else if (chosen.some((id) => !known.has(id))) {
    errors.tunnelIds = strings.errorAppTunnelMissing;
  } else if (chosen.length > MAX_APP_TUNNELS) {
    errors.tunnelIds = strings.errorMaxAppTunnels;
  } else if (!chosen.some((id) => tunnels.find((t) => t.id === id)?.dialable)) {
    // 选了隧道但一条都拨不出去：与其让用户点“启动”后才发现，不如在这里拦住。
    errors.tunnelIds = strings.errorNoDialableTunnel;
  }

  const localPort = parsePort(form.localPort);
  if (localPort === null || localPort < 1024 || localPort > 65535) {
    errors.localPort = strings.errorLocalPort;
  }

  if (form.remoteHost.trim() === "" || /\s/.test(form.remoteHost)) {
    errors.remoteHost = strings.errorRemoteHostRequired;
  }
  const remotePort = parsePort(form.remotePort);
  if (remotePort === null || remotePort < 1 || remotePort > 65535) {
    errors.remotePort = strings.errorRemotePort;
  }

  const browserUrl = form.browserUrl.trim();
  if (
    browserUrl !== "" &&
    !browserUrl.toLowerCase().startsWith("http://") &&
    !browserUrl.toLowerCase().startsWith("https://")
  ) {
    errors.browserUrl = strings.errorBrowserUrl;
  }

  return errors;
}

function credentialInput(form: JumpForm | TunnelForm) {
  return {
    authType: form.authType,
    password: form.passwordCleared
      ? ""
      : form.passwordInput !== ""
        ? form.passwordInput
        : null,
    privateKey: form.privateKeyCleared
      ? ""
      : form.privateKeyInput !== ""
        ? form.privateKeyInput
        : null,
    passphrase: form.passphraseCleared
      ? ""
      : form.passphraseInput !== ""
        ? form.passphraseInput
        : null,
    privateKeyPath:
      form.authType === "private_key" && usesLocalKey(form)
        ? (form.privateKeyPath ?? "").trim()
        : null,
  };
}

export function toJumpInput(jump: JumpForm): JumpInput {
  return {
    id: jump.id,
    label: jump.label.trim(),
    host: jump.host.trim(),
    port: Number(jump.port),
    username: jump.username.trim(),
    credentials: credentialInput(jump),
  };
}

export function toTunnelInput(form: TunnelForm): TunnelInput {
  return {
    id: form.id,
    name: form.name.trim(),
    host: form.host.trim(),
    port: Number(form.port),
    username: form.username.trim(),
    credentials: credentialInput(form),
    jumps: form.jumps.map(toJumpInput),
    autoReconnect: form.autoReconnect,
    keepAliveSeconds: Number(form.keepAliveSeconds),
    connectTimeoutSeconds: Number(form.connectTimeoutSeconds),
    strictHostKey: form.strictHostKey,
  };
}

export function toAppInput(form: AppForm): AppInput {
  return {
    id: form.id,
    name: form.name.trim(),
    groupId: form.groupId,
    // 去重但保持顺序：第一条就是首选隧道。
    tunnelIds: form.tunnelIds.filter(
      (id, index) => form.tunnelIds.indexOf(id) === index,
    ),
    localPort: Number(form.localPort),
    remoteHost: form.remoteHost.trim(),
    remotePort: Number(form.remotePort),
    browserUrl: form.browserUrl.trim(),
    autoStartOnBoot: form.autoStartOnBoot,
  };
}

/** 一条隧道的链路预览：跳板 → SSH 服务器。 */
export function tunnelChainPreview(tunnel: TunnelForm): string {
  const hops = tunnel.jumps.map((jump) => jump.label || jump.host || "?");
  hops.push(tunnel.host || strings.editorServerPlaceholder);
  return hops.join("  →  ");
}

/**
 * 编辑器顶部的链路预览：当前隧道自己的接入路径。没有地址时给出占位文案，
 * 让新表单里也有可读的标题。
 */
export function chainPreview(form: TunnelForm): string {
  if (form.host.trim() === "" && form.jumps.length === 0) {
    return strings.editorNoEndpoint;
  }
  return tunnelChainPreview(form);
}

/**
 * 应用要尝试的隧道，按应用里的顺序；引用不到（已删除）的跳过。
 * 卡片、错误详情页都用它描述“会走哪几条路”。
 */
export function appTunnels(
  form: Pick<AppForm, "tunnelIds">,
  tunnels: TunnelView[],
): TunnelView[] {
  const byId = new Map(tunnels.map((tunnel) => [tunnel.id, tunnel]));
  return form.tunnelIds
    .map((id) => byId.get(id))
    .filter((tunnel): tunnel is TunnelView => tunnel !== undefined);
}

/** 应用选择的隧道里有没有一条现在就能拨号。 */
export function appHasDialableTunnel(
  form: Pick<AppForm, "tunnelIds">,
  tunnels: TunnelView[],
): boolean {
  return appTunnels(form, tunnels).some((tunnel) => tunnel.dialable);
}
