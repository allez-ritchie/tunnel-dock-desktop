import type { AppStatus, AppView, GroupView, TunnelView } from "./api";

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

  if (!appHasKnownTunnel(app, tunnels)) {
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
      tunnel: null,
      tunnelFailures: [],
      updatedAt: Date.now(),
    }
  );
}

/** 应用引用的隧道里至少还有一条存在。 */
export function appHasKnownTunnel(
  app: Pick<AppView, "tunnelIds">,
  tunnels: TunnelView[],
): boolean {
  if (app.tunnelIds.length === 0) return false;
  return app.tunnelIds.some((id) => tunnels.some((tunnel) => tunnel.id === id));
}

/**
 * 应用当前实际使用的隧道：优先取运行状态里记录的，其次按声明顺序找第一条
 * 还存在的。返回 `undefined` 说明应用引用的隧道全都被删了。
 */
export function activeTunnel(
  app: Pick<AppView, "tunnelIds">,
  tunnels: TunnelView[],
  status?: AppStatus,
): TunnelView | undefined {
  const live = status?.tunnel?.tunnelId;
  if (live !== undefined) {
    const found = tunnels.find((tunnel) => tunnel.id === live);
    if (found) return found;
  }
  return appTunnels(app, tunnels)[0];
}

/** 应用按顺序持有的隧道，引用不到的跳过。 */
export function appTunnels(
  app: Pick<AppView, "tunnelIds">,
  tunnels: TunnelView[],
): TunnelView[] {
  return app.tunnelIds
    .map((id) => tunnels.find((tunnel) => tunnel.id === id))
    .filter((tunnel): tunnel is TunnelView => tunnel !== undefined);
}

/** 首选隧道：应用声明顺序里的第一条（不是实际在用的那条）。 */
export function primaryTunnel(
  app: Pick<AppView, "tunnelIds">,
  tunnels: TunnelView[],
): TunnelView | undefined {
  return appTunnels(app, tunnels)[0];
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
    tunnel: null,
    tunnelFailures: [],
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

/** 隧道自身的接入信息是否完整（地址 / 用户名 / 端口 / 凭据）。 */
export function tunnelHasDialableEndpoint(
  tunnel: TunnelView | undefined,
): boolean {
  return tunnel !== undefined && tunnel.dialable;
}

/**
 * 该应用当前是否靠“下一条隧道”在工作：用来在卡片上提示
 * “首选隧道不可用，已切换到 X”。用首选隧道本身时说明没发生回退。
 */
export function usedFallback(
  status: AppStatus,
  app: Pick<AppView, "tunnelIds">,
): boolean {
  // 只有配置了多个入口才谈得上“切换”：单入口失败后重连成功不算回退。
  if (app.tunnelIds.length < 2) return false;
  if (status.tunnelFailures.length === 0) return false;
  if (status.phase !== "connected") return false;
  const preferred = app.tunnelIds[0];
  return status.tunnel?.tunnelId !== preferred;
}
