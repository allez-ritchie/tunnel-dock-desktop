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
}

export interface TunnelForm {
  id: string | null;
  name: string;
  sshHost: string;
  sshPort: string;
  sshUser: string;
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
  tunnelId: string;
  localPort: string;
  remoteHost: string;
  remotePort: string;
  browserUrl: string;
  autoStartOnBoot: boolean;
}

export const MAX_JUMPS = 8;

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
  };
}

export function tunnelFormFrom(tunnel?: TunnelView): TunnelForm {
  return {
    id: tunnel?.id ?? null,
    name: tunnel?.name ?? "",
    sshHost: tunnel?.sshHost ?? "",
    sshPort: String(tunnel?.sshPort ?? 22),
    sshUser: tunnel?.sshUser ?? "root",
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
  defaults?: { groupId?: string; tunnelId?: string; localPort?: number },
): AppForm {
  return {
    id: app?.id ?? null,
    name: app?.name ?? "",
    groupId: app?.groupId ?? defaults?.groupId ?? "default",
    tunnelId: app?.tunnelId ?? defaults?.tunnelId ?? "",
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

export function validateTunnel(form: TunnelForm): FormErrors {
  const errors: FormErrors = {};
  const name = form.name.trim();
  if (name === "") errors.name = strings.errorNameRequired;
  else if ([...name].length > 40) errors.name = strings.errorNameTooLong;

  if (form.sshHost.trim() === "" || /\s/.test(form.sshHost)) {
    errors.sshHost = strings.errorSshHostRequired;
  }
  const sshPort = parsePort(form.sshPort);
  if (sshPort === null || sshPort < 1 || sshPort > 65535) {
    errors.sshPort = strings.errorSshPort;
  }
  if (form.sshUser.trim() === "") errors.sshUser = strings.errorUsernameRequired;

  if (form.authType === "password") {
    if (!hasSavedPassword(form) && form.passwordInput === "") {
      errors.password = strings.errorPasswordRequired;
    }
  } else if (!hasSavedPrivateKey(form) && form.privateKeyInput.trim() === "") {
    errors.privateKey = strings.errorPrivateKeyRequired;
  }

  const keepAlive = parsePort(form.keepAliveSeconds);
  if (keepAlive === null || keepAlive < 5 || keepAlive > 300) {
    errors.keepAliveSeconds = strings.errorKeepalive;
  }
  const timeout = parsePort(form.connectTimeoutSeconds);
  if (timeout === null || timeout < 5 || timeout > 120) {
    errors.connectTimeoutSeconds = strings.errorTimeout;
  }

  if (form.jumps.length > MAX_JUMPS) {
    errors.jumps = strings.errorMaxJumps;
  } else if (form.jumps.some((jump) => jumpError(jump) !== null)) {
    errors.jumps = strings.errorJumpInvalid;
  }

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
  if (!tunnels.some((tunnel) => tunnel.id === form.tunnelId)) {
    errors.tunnelId = strings.errorTunnelRequired;
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
    sshHost: form.sshHost.trim(),
    sshPort: Number(form.sshPort),
    sshUser: form.sshUser.trim(),
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
    tunnelId: form.tunnelId,
    localPort: Number(form.localPort),
    remoteHost: form.remoteHost.trim(),
    remotePort: Number(form.remotePort),
    browserUrl: form.browserUrl.trim(),
    autoStartOnBoot: form.autoStartOnBoot,
  };
}

export function chainPreview(form: TunnelForm): string {
  const hops = form.jumps.map((jump) => jump.label || jump.host || "?");
  hops.push(form.sshHost || "SSH 服务器");
  return hops.join("  →  ");
}
