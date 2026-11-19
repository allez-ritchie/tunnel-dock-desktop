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
