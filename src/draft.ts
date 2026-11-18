import type {
  AuthType,
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
  passphraseInput: string;
  passphraseCleared: boolean;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
}

export interface TunnelForm {
  id: string | null;
  name: string;
  localPort: string;
  remoteHost: string;
  remotePort: string;
  sshHost: string;
  sshPort: string;
  sshUser: string;
  authType: AuthType;
  passwordInput: string;
  passwordCleared: boolean;
  privateKeyInput: string;
  privateKeyCleared: boolean;
  passphraseInput: string;
  passphraseCleared: boolean;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
  jumps: JumpForm[];
  autoReconnect: boolean;
  autoStartOnBoot: boolean;
  strictHostKey: boolean;
  keepAliveSeconds: string;
  connectTimeoutSeconds: string;
  browserUrl: string;
}

export const MAX_JUMPS = 8;

export function nextPort(tunnels: TunnelView[]): number {
  const used = new Set(tunnels.map((tunnel) => tunnel.localPort));
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
    passphraseInput: "",
    passphraseCleared: false,
    hasPassword: jump?.credentials.hasPassword ?? false,
    hasPrivateKey: jump?.credentials.hasPrivateKey ?? false,
    hasPassphrase: jump?.credentials.hasPassphrase ?? false,
  };
}

export function formFrom(tunnel?: TunnelView, suggestedPort = 18080): TunnelForm {
  return {
    id: tunnel?.id ?? null,
    name: tunnel?.name ?? "",
    localPort: String(tunnel?.localPort ?? suggestedPort),
    remoteHost: tunnel?.remoteHost ?? "127.0.0.1",
    remotePort: String(tunnel?.remotePort ?? 80),
    sshHost: tunnel?.sshHost ?? "",
    sshPort: String(tunnel?.sshPort ?? 22),
    sshUser: tunnel?.sshUser ?? "root",
    authType: tunnel?.credentials.authType ?? "password",
    passwordInput: "",
    passwordCleared: false,
    privateKeyInput: "",
    privateKeyCleared: false,
    passphraseInput: "",
    passphraseCleared: false,
    hasPassword: tunnel?.credentials.hasPassword ?? false,
    hasPrivateKey: tunnel?.credentials.hasPrivateKey ?? false,
    hasPassphrase: tunnel?.credentials.hasPassphrase ?? false,
    jumps: tunnel?.jumps.map((jump) => jumpFormFrom(jump)) ?? [],
    autoReconnect: tunnel?.autoReconnect ?? true,
    autoStartOnBoot: tunnel?.autoStartOnBoot ?? false,
    strictHostKey: tunnel?.strictHostKey ?? true,
    keepAliveSeconds: String(tunnel?.keepAliveSeconds ?? 30),
    connectTimeoutSeconds: String(tunnel?.connectTimeoutSeconds ?? 15),
    browserUrl: tunnel?.browserUrl ?? "",
  };
}

export function signature(form: TunnelForm): string {
  return JSON.stringify(form);
}

export function hasSavedPassword(form: JumpForm | TunnelForm): boolean {
  return form.hasPassword && !form.passwordCleared;
}

export function hasSavedPrivateKey(form: JumpForm | TunnelForm): boolean {
  return form.hasPrivateKey && !form.privateKeyCleared;
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

export function validate(form: TunnelForm, others: TunnelView[]): FormErrors {
  const errors: FormErrors = {};
  const name = form.name.trim();
  if (name === "") errors.name = strings.errorNameRequired;
  else if ([...name].length > 40) errors.name = strings.errorNameTooLong;

  const localPort = parsePort(form.localPort);
  if (localPort === null || localPort < 1024 || localPort > 65535) {
    errors.localPort = strings.errorLocalPort;
  } else {
    const conflict = others.find(
      (tunnel) => tunnel.id !== form.id && tunnel.localPort === localPort,
    );
    if (conflict) errors.localPort = strings.errorLocalPortInUse(conflict.name);
  }

  if (form.remoteHost.trim() === "" || /\s/.test(form.remoteHost)) {
    errors.remoteHost = strings.errorRemoteHostRequired;
  }
  const remotePort = parsePort(form.remotePort);
  if (remotePort === null || remotePort < 1 || remotePort > 65535) {
    errors.remotePort = strings.errorRemotePort;
  }
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

  const browserUrl = form.browserUrl.trim();
  if (
    browserUrl !== "" &&
    !browserUrl.toLowerCase().startsWith("http://") &&
    !browserUrl.toLowerCase().startsWith("https://")
  ) {
    errors.browserUrl = strings.errorBrowserUrl;
  }

  if (form.jumps.length > MAX_JUMPS) {
    errors.jumps = strings.errorMaxJumps;
  } else if (form.jumps.some((jump) => jumpError(jump) !== null)) {
    errors.jumps = strings.errorJumpInvalid;
  }

  return errors;
}

export function toJumpInput(jump: JumpForm): JumpInput {
  return {
    id: jump.id,
    label: jump.label.trim(),
    host: jump.host.trim(),
    port: Number(jump.port),
    username: jump.username.trim(),
    credentials: {
      authType: jump.authType,
      password: jump.passwordCleared
        ? ""
        : jump.passwordInput !== ""
          ? jump.passwordInput
          : null,
      privateKey: jump.privateKeyCleared
        ? ""
        : jump.privateKeyInput !== ""
          ? jump.privateKeyInput
          : null,
      passphrase: jump.passphraseCleared
        ? ""
        : jump.passphraseInput !== ""
          ? jump.passphraseInput
          : null,
    },
  };
}

export function toInput(form: TunnelForm): TunnelInput {
  return {
    id: form.id,
    name: form.name.trim(),
    localHost: "127.0.0.1",
    localPort: Number(form.localPort),
    remoteHost: form.remoteHost.trim(),
    remotePort: Number(form.remotePort),
    sshHost: form.sshHost.trim(),
    sshPort: Number(form.sshPort),
    sshUser: form.sshUser.trim(),
    credentials: {
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
    },
    jumps: form.jumps.map(toJumpInput),
    autoReconnect: form.autoReconnect,
    autoStartOnBoot: form.autoStartOnBoot,
    keepAliveSeconds: Number(form.keepAliveSeconds),
    connectTimeoutSeconds: Number(form.connectTimeoutSeconds),
    strictHostKey: form.strictHostKey,
    browserUrl: form.browserUrl.trim(),
  };
}

export function chainPreview(form: TunnelForm): string {
  const hops = form.jumps.map((jump) => jump.label || jump.host || "?");
  hops.push(form.sshHost || "SSH 服务器");
  return hops.join("  →  ");
}
