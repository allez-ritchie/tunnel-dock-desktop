import type {
  AppStatus,
  AppView,
  EndpointSelection,
  GroupView,
  TunnelView,
} from "./api";

/**
 * Resolves the status to render for one application.
 *
 * The backend is the source of truth for live state, but a definition change
 * (a deleted tunnel or group) can land before the matching event arrives, and a
 * brand-new application has no status at all. Both cases are derived here so the
 * card never shows "已停止" for an application that provably cannot start.
 */
export function resolveAppStatus(
  app: AppView,
  tunnels: TunnelView[],
  groups: GroupView[],
  statuses: Record<string, AppStatus>,
): AppStatus {
  const existing = statuses[app.id];
  // A blocked status is sticky: keep explaining why until the app is restarted.
  if (existing && existing.blockReason !== "none") return existing;

  if (!tunnels.some((tunnel) => tunnel.id === app.tunnelId)) {
    return staticBlocked(app, "tunnel_deleted");
  }
  if (!groups.some((group) => group.id === app.groupId)) {
    return staticBlocked(app, "group_missing");
  }
  return (
    existing ?? {
      appId: app.id,
      phase: app.desiredOn ? "connecting" : "idle",
      blockReason: "none",
      attempt: 0,
      nextRetrySeconds: null,
      connectedAt: null,
      localPort: null,
      errorCode: null,
      errorDetail: null,
      securityAlert: false,
      endpoint: null,
      endpointFailures: [],
      updatedAt: Date.now(),
    }
  );
}

function staticBlocked(
  app: AppView,
  reason: "tunnel_deleted" | "group_missing",
): AppStatus {
  return {
    appId: app.id,
    phase: "error",
    blockReason: reason,
    attempt: 0,
    nextRetrySeconds: null,
    connectedAt: null,
    localPort: app.localPort,
    errorCode: reason,
    errorDetail: null,
    securityAlert: false,
    endpoint: null,
    endpointFailures: [],
    updatedAt: Date.now(),
  };
}

/** True when the application cannot be started and needs an explanation. */
export function appUnavailable(
  tunnel: TunnelView | undefined,
  group: GroupView | undefined,
  status: AppStatus,
): boolean {
  return (
    !tunnel ||
    !group ||
    status.blockReason !== "none" ||
    status.phase === "error"
  );
}

/**
 * 隧道是否有一条现在就能拨号的线路。没有的话应用同样“不可用”，
 * 但原因不是连接失败而是配置缺失，界面据此给出可执行的提示。
 */
export function tunnelHasDialableEndpoint(tunnel: TunnelView | undefined): boolean {
  return tunnel !== undefined && tunnel.endpoints.some((item) => item.dialable);
}

/** 线路名，没有线路时回落到占位符。 */
export function endpointLabel(endpoint: EndpointSelection | null | undefined): string {
  return endpoint?.label ?? "—";
}

/**
 * 该应用当前是否靠“下一条线路”在工作：用来在卡片上提示
 * “首选线路不可用，已切换到 X”。切到自己就说明没发生回退。
 */
export function usedFallback(
  status: AppStatus,
  tunnel: TunnelView | undefined,
): boolean {
  if (status.endpointFailures.length === 0) return false;
  if (status.phase !== "connected") return false;
  const preferred = tunnel?.preferredEndpointId ?? null;
  if (preferred === null) return false;
  return status.endpoint?.endpointId !== preferred;
}

/** 首选线路本身，界面在“回退”提示里要引用它的名字。 */
export function preferredEndpoint(tunnel: TunnelView | undefined) {
  if (!tunnel) return undefined;
  return (
    tunnel.endpoints.find((item) => item.id === tunnel.preferredEndpointId) ??
    tunnel.endpoints[0]
  );
}
