import { describe, expect, it } from "vitest";
import type {
  AppStatus,
  AppView,
  GroupView,
  TunnelView,
} from "../src/api";
import {
  activeTunnel,
  appHasKnownTunnel,
  appTunnels,
  appUnavailable,
  primaryTunnel,
  resolveAppStatus,
  tunnelHasDialableEndpoint,
  usedFallback,
} from "../src/status";

function tunnel(overrides: Partial<TunnelView> = {}): TunnelView {
  return {
    id: "t1",
    name: "外网",
    host: "example.com",
    port: 22,
    username: "ops",
    credentials: {
      authType: "password",
      hasPassword: true,
      hasPrivateKey: false,
      hasPassphrase: false,
    },
    jumps: [],
    autoReconnect: true,
    keepAliveSeconds: 30,
    connectTimeoutSeconds: 15,
    strictHostKey: true,
    createdAt: 1,
    updatedAt: 1,
    endpoint: "ops@example.com:22",
    chainSummary: "example.com",
    dialable: true,
    ...overrides,
  };
}

function app(overrides: Partial<AppView> = {}): AppView {
  return {
    id: "a1",
    name: "Web",
    groupId: "default",
    tunnelIds: ["t1"],
    localPort: 18321,
    remoteHost: "127.0.0.1",
    remotePort: 9191,
    browserUrl: "",
    browserOpenUrl: "http://127.0.0.1:18321/",
    localUrl: "http://127.0.0.1:18321/",
    autoStartOnBoot: false,
    desiredOn: true,
    createdAt: 1,
    updatedAt: 1,
    ...overrides,
  };
}

function group(overrides: Partial<GroupView> = {}): GroupView {
  return {
    id: "default",
    name: "default",
    isDefault: true,
    createdAt: 0,
    ...overrides,
  };
}

function status(overrides: Partial<AppStatus> = {}): AppStatus {
  return {
    appId: "a1",
    phase: "connected",
    blockReason: "none",
    attempt: 0,
    nextRetrySeconds: null,
    connectedAt: 1,
    localPort: 18321,
    errorCode: null,
    errorDetail: null,
    securityAlert: false,
    tunnel: null,
    tunnelFailures: [],
    updatedAt: 1,
    ...overrides,
  };
}

const home = tunnel({ id: "home", name: "家里", host: "192.168.1.10" });
const away = tunnel({ id: "away", name: "外网", host: "frp.example.com" });

describe("resolveAppStatus", () => {
  it("derives a connecting placeholder for a brand-new desired app", () => {
    const resolved = resolveAppStatus(app(), [tunnel()], [group()], {});
    expect(resolved.phase).toBe("connecting");
    expect(resolved.tunnelFailures).toEqual([]);
    expect(resolved.tunnel).toBeNull();
  });

  it("shows idle when the app is not desired", () => {
    expect(
      resolveAppStatus(app({ desiredOn: false }), [tunnel()], [group()], {}).phase,
    ).toBe("idle");
  });

  it("reports deleted tunnels even before an event arrives", () => {
    const resolved = resolveAppStatus(app(), [], [group()], {});
    expect(resolved.blockReason).toBe("tunnel_deleted");
    expect(resolved.phase).toBe("error");
  });

  it("does not block an app that still has one surviving tunnel", () => {
    const resolved = resolveAppStatus(
      app({ tunnelIds: ["gone", "away"] }),
      [away],
      [group()],
      {},
    );
    expect(resolved.blockReason).toBe("none");
  });

  it("reports a missing group", () => {
    expect(resolveAppStatus(app(), [tunnel()], [], {}).blockReason).toBe(
      "group_missing",
    );
  });

  it("keeps a sticky blocked status from the backend", () => {
    const blocked = status({ blockReason: "tunnel_down", phase: "reconnecting" });
    expect(
      resolveAppStatus(app(), [tunnel()], [group()], { a1: blocked }),
    ).toBe(blocked);
  });

  it("passes the live tunnel information through", () => {
    const live = status({
      tunnel: { tunnelId: "away", name: "外网", endpoint: "ops@frp:22" },
    });
    const resolved = resolveAppStatus(app(), [tunnel()], [group()], { a1: live });
    expect(resolved.tunnel?.name).toBe("外网");
  });
});

describe("appHasKnownTunnel", () => {
  it("needs at least one surviving tunnel", () => {
    expect(appHasKnownTunnel({ tunnelIds: ["home"] }, [home])).toBe(true);
    expect(appHasKnownTunnel({ tunnelIds: ["gone"] }, [home])).toBe(false);
    expect(appHasKnownTunnel({ tunnelIds: [] }, [home])).toBe(false);
  });
});

describe("appTunnels", () => {
  const tunnels = [home, away];

  it("keeps the declared order", () => {
    expect(appTunnels({ tunnelIds: ["away", "home"] }, tunnels).map((t) => t.id)).toEqual([
      "away",
      "home",
    ]);
  });

  it("skips references that no longer resolve", () => {
    expect(appTunnels({ tunnelIds: ["gone", "away"] }, tunnels).map((t) => t.id)).toEqual([
      "away",
    ]);
  });

  it("exposes the primary tunnel as the first declared one", () => {
    expect(primaryTunnel({ tunnelIds: ["away", "home"] }, tunnels)?.id).toBe("away");
    expect(primaryTunnel({ tunnelIds: [] }, tunnels)).toBeUndefined();
  });
});

describe("activeTunnel", () => {
  const tunnels = [home, away];

  it("prefers the tunnel reported by the live status", () => {
    const live = status({
      tunnel: { tunnelId: "away", name: "外网", endpoint: "ops@frp:22" },
    });
    expect(activeTunnel({ tunnelIds: ["home", "away"] }, tunnels, live)?.id).toBe(
      "away",
    );
  });

  it("falls back to the first declared tunnel before the status arrives", () => {
    expect(activeTunnel({ tunnelIds: ["home", "away"] }, tunnels)?.id).toBe("home");
  });

  it("is undefined when every tunnel is gone", () => {
    expect(activeTunnel({ tunnelIds: ["gone"] }, tunnels)).toBeUndefined();
  });
});

describe("appUnavailable", () => {
  it("is true without a tunnel or group", () => {
    expect(appUnavailable(undefined, group(), status())).toBe(true);
    expect(appUnavailable(tunnel(), undefined, status())).toBe(true);
  });

  it("is true for an errored or blocked status", () => {
    expect(appUnavailable(tunnel(), group(), status({ phase: "error" }))).toBe(true);
    expect(
      appUnavailable(tunnel(), group(), status({ blockReason: "port_conflict" })),
    ).toBe(true);
  });

  it("is false for a healthy app", () => {
    expect(appUnavailable(tunnel(), group(), status())).toBe(false);
  });
});

describe("tunnelHasDialableEndpoint", () => {
  it("is false when the tunnel is missing", () => {
    expect(tunnelHasDialableEndpoint(undefined)).toBe(false);
  });

  it("follows the backend dialable flag", () => {
    expect(tunnelHasDialableEndpoint(tunnel({ dialable: true }))).toBe(true);
    expect(tunnelHasDialableEndpoint(tunnel({ dialable: false }))).toBe(false);
  });
});

describe("usedFallback", () => {
  const appTwoTunnels = { tunnelIds: ["home", "away"] };

  it("is false without failures", () => {
    expect(usedFallback(status(), appTwoTunnels)).toBe(false);
  });

  it("is false when the preferred tunnel is in use", () => {
    const live = status({
      tunnel: { tunnelId: "home", name: "家里", endpoint: "a" },
      tunnelFailures: [
        { tunnelId: "other", name: "x", code: "timeout", detail: null },
      ],
    });
    expect(usedFallback(live, appTwoTunnels)).toBe(false);
  });

  it("is true when a fallback tunnel carries the traffic", () => {
    const live = status({
      tunnel: { tunnelId: "away", name: "外网", endpoint: "b" },
      tunnelFailures: [{ tunnelId: "home", name: "家里", code: "timeout", detail: null }],
    });
    expect(usedFallback(live, appTwoTunnels)).toBe(true);
  });

  it("is false before the connection is up", () => {
    const reconnecting = status({
      phase: "reconnecting",
      tunnel: { tunnelId: "away", name: "外网", endpoint: "b" },
      tunnelFailures: [{ tunnelId: "home", name: "家里", code: "timeout", detail: null }],
    });
    expect(usedFallback(reconnecting, appTwoTunnels)).toBe(false);
  });

  it("is false for a single-tunnel app (nothing to fall back to)", () => {
    const live = status({
      tunnel: { tunnelId: "other", name: "别的", endpoint: "b" },
      tunnelFailures: [{ tunnelId: "home", name: "家里", code: "timeout", detail: null }],
    });
    expect(usedFallback(live, { tunnelIds: ["home"] })).toBe(false);
  });
});
