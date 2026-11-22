import type {
  AppInput,
  AppView,
  AuthType,
  EndpointInput,
  EndpointView,
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
 * 一条 SSH 线路的可编辑状态。一条隧道可以有多条线路，它们是“到达同一台
 * SSH 服务器的不同入口”，应用看到的本地端口始终不变。
 */
export interface EndpointForm {
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
  privateKeyPath: string | null;
  passphraseInput: string;
  passphraseCleared: boolean;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
  jumps: JumpForm[];
  /** 只在局域网（有线 / Wi-Fi）可用，例如家里的那台服务器。 */
  wifiOnly: boolean;
  enabled: boolean;
}

export interface TunnelForm {
  id: string | null;
  name: string;
  endpoints: EndpointForm[];
  preferredEndpointId: string | null;
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
export const MAX_ENDPOINTS = 8;

export function nextPort(apps: AppView[]): number {
  const used = new Set(apps.map((app) => app.localPort));
  for (let port = 18080; port <= 65535; port += 1) {
    if (!used.has(port)) return port;
  }
  return 18080;
}

/** "线路 1"、"线路 2"……跳过已经用掉的名字。 */
export function nextEndpointLabel(existing: string[]): string {
  let index = existing.length + 1;
  while (existing.includes(strings.endpointDefaultLabel(index))) index += 1;
  return strings.endpointDefaultLabel(index);
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

export function endpointFormFrom(
  endpoint?: EndpointView,
  defaultLabel = "",
): EndpointForm {
  return {
    id: endpoint?.id ?? null,
    label: endpoint?.label ?? defaultLabel,
    host: endpoint?.host ?? "",
    port: String(endpoint?.port ?? 22),
    username: endpoint?.username ?? "root",
    authType: endpoint?.credentials.authType ?? "password",
    passwordInput: "",
    passwordCleared: false,
    privateKeyInput: "",
    privateKeyCleared: false,
    privateKeyPath: null,
    passphraseInput: "",
    passphraseCleared: false,
    hasPassword: endpoint?.credentials.hasPassword ?? false,
    hasPrivateKey: endpoint?.credentials.hasPrivateKey ?? false,
    hasPassphrase: endpoint?.credentials.hasPassphrase ?? false,
    jumps: endpoint?.jumps.map((jump) => jumpFormFrom(jump)) ?? [],
    wifiOnly: endpoint?.wifiOnly ?? false,
    enabled: endpoint?.enabled ?? true,
  };
}

export function tunnelFormFrom(tunnel?: TunnelView): TunnelForm {
  return {
    id: tunnel?.id ?? null,
    name: tunnel?.name ?? "",
    endpoints:
      tunnel?.endpoints.map((endpoint) => endpointFormFrom(endpoint)) ?? [],
    preferredEndpointId: tunnel?.preferredEndpointId ?? null,
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

export function hasSavedPassword(form: JumpForm | EndpointForm): boolean {
  return form.hasPassword && !form.passwordCleared;
}

export function hasSavedPrivateKey(form: JumpForm | EndpointForm): boolean {
  return (form.hasPrivateKey && !form.privateKeyCleared) || usesLocalKey(form);
}

/** 是否选择了 `~/.ssh` 里的私钥文件（保存在本机，不在表单里传内容）。 */
export function usesLocalKey(form: JumpForm | EndpointForm): boolean {
  return (form.privateKeyPath ?? "").trim() !== "";
}

export function hasSavedPassphrase(form: JumpForm | EndpointForm): boolean {
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

/** 一条线路自身是否完整，返回第一条错误信息或 `null`。 */
export function endpointError(endpoint: EndpointForm): string | null {
  if ([...endpoint.label.trim()].length > 40) {
    return strings.errorEndpointLabelTooLong;
  }
  if (endpoint.host.trim() === "" || /\s/.test(endpoint.host)) {
    return strings.errorEndpointHostRequired;
  }
  const port = parsePort(endpoint.port);
  if (port === null || port < 1 || port > 65535) {
    return strings.errorEndpointPort;
  }
  if (endpoint.username.trim() === "") {
    return strings.errorEndpointUsernameRequired;
  }
  if (endpoint.authType === "password") {
    if (!hasSavedPassword(endpoint) && endpoint.passwordInput === "") {
      return strings.errorEndpointPasswordRequired;
    }
  } else if (
    !hasSavedPrivateKey(endpoint) &&
    endpoint.privateKeyInput.trim() === ""
  ) {
    return strings.errorEndpointPrivateKeyRequired;
  }
  if (endpoint.jumps.length > MAX_JUMPS) {
    return strings.errorMaxJumps;
  }
  if (endpoint.jumps.some((jump) => jumpError(jump) !== null)) {
    return strings.errorJumpInvalid;
  }
  return null;
}

/** 线路名不允许重复，否则日志与卡片里分不清是哪条路。 */
export function duplicateEndpointLabel(
  endpoints: EndpointForm[],
  index: number,
): boolean {
  const label = endpoints[index].label.trim();
  if (label === "") return false;
  return endpoints.some(
    (endpoint, other) => other !== index && endpoint.label.trim() === label,
  );
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

  if (form.endpoints.length === 0) {
    errors.endpoints = strings.errorEndpointRequired;
  } else if (form.endpoints.length > MAX_ENDPOINTS) {
    errors.endpoints = strings.errorMaxEndpoints;
  } else if (!form.endpoints.some((endpoint) => endpoint.enabled)) {
    errors.endpoints = strings.errorEndpointAllDisabled;
  } else {
    for (const [index, endpoint] of form.endpoints.entries()) {
      if (duplicateEndpointLabel(form.endpoints, index)) {
        errors.endpoints = strings.errorEndpointLabelDuplicate;
        break;
      }
      const error = endpointError(endpoint);
      if (error !== null) {
        errors.endpoints = error;
        break;
      }
    }
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

function credentialInput(form: JumpForm | EndpointForm) {
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

export function toEndpointInput(endpoint: EndpointForm): EndpointInput {
  return {
    id: endpoint.id,
    label: endpoint.label.trim(),
    host: endpoint.host.trim(),
    port: Number(endpoint.port),
    username: endpoint.username.trim(),
    credentials: credentialInput(endpoint),
    jumps: endpoint.jumps.map(toJumpInput),
    wifiOnly: endpoint.wifiOnly,
    enabled: endpoint.enabled,
  };
}

export function toTunnelInput(form: TunnelForm): TunnelInput {
  // 首选线路必须确实存在；指向一条已被删掉的线路时回落到自动。
  const preferred =
    form.preferredEndpointId !== null &&
    form.endpoints.some((endpoint) => endpoint.id === form.preferredEndpointId)
      ? form.preferredEndpointId
      : null;
  return {
    id: form.id,
    name: form.name.trim(),
    endpoints: form.endpoints.map(toEndpointInput),
    autoReconnect: form.autoReconnect,
    keepAliveSeconds: Number(form.keepAliveSeconds),
    connectTimeoutSeconds: Number(form.connectTimeoutSeconds),
    strictHostKey: form.strictHostKey,
    preferredEndpointId: preferred,
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

/** 一条线路的链路预览：跳板 → SSH 服务器。 */
export function endpointChainPreview(endpoint: EndpointForm): string {
  const hops = endpoint.jumps.map((jump) => jump.label || jump.host || "?");
  hops.push(endpoint.host || strings.editorServerPlaceholder);
  return hops.join("  →  ");
}

/**
 * 编辑器顶部的链路预览。布局与后端 `primary_endpoint()` 一致：首选线路优先，
 * 否则第一条线路；一条线路都没有时给出占位文案。
 */
export function chainPreview(form: TunnelForm): string {
  const preferred =
    form.endpoints.find(
      (endpoint) => endpoint.id !== null && endpoint.id === form.preferredEndpointId,
    ) ?? form.endpoints[0];
  if (!preferred) return strings.editorNoEndpoint;
  const preview = endpointChainPreview(preferred);
  return form.endpoints.length > 1
    ? `${preview}  ${strings.endpointCountSuffix(form.endpoints.length)}`
    : preview;
}
