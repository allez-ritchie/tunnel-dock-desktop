import { describe, expect, it } from "vitest";
import type {
  AppStatus,
  AppView,
  EndpointView,
  GroupView,
  TunnelView,
} from "../src/api";
import {
  appUnavailable,
  preferredEndpoint,
  resolveAppStatus,
  tunnelHasDialableEndpoint,
  usedFallback,
} from "../src/status";

function endpoint(overrides: Partial<EndpointView> = {}): EndpointView {
  return {
    id: "e1",
    label: "外网",
    displayName: "外网",
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
    wifiOnly: false,
    enabled: true,
    endpoint: "ops@example.com:22",
    chainSummary: "example.com",
    dialable: true,
    ...overrides,
  };
}

function tunnel(overrides: Partial<TunnelView> = {}): TunnelView {
  return {
    id: "t1",
    name: "NAS",
    endpoints: [endpoint()],
    autoReconnect: true,
    keepAliveSeconds: 30,
    connectTimeoutSeconds: 15,
    strictHostKey: true,
    preferredEndpointId: "e1",
    createdAt: 1,
    updatedAt: 1,
    endpoint: "ops@example.com:22",
    chainSummary: "example.com",
    dialableCount: 1,
    ...overrides,
  };
}

function app(overrides: Partial<AppView> = {}): AppView {
  return {
    id: "a1",
    name: "Web",
    groupId: "default",
    tunnelId: "t1",
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
    endpoint: null,
    endpointFailures: [],
    updatedAt: 1,
    ...overrides,
  };
}

describe("resolveAppStatus", () => {
  it("derives a connecting placeholder for a brand-new desired app", () => {
    const resolved = resolveAppStatus(app(), [tunnel()], [group()], {});
    expect(resolved.phase).toBe("connecting");
    expect(resolved.endpointFailures).toEqual([]);
    expect(resolved.endpoint).toBeNull();
  });

  it("shows idle when the app is not desired", () => {
    expect(
      resolveAppStatus(app({ desiredOn: false }), [tunnel()], [group()], {}).phase,
    ).toBe("idle");
  });

  it("reports a deleted tunnel even before an event arrives", () => {
    const resolved = resolveAppStatus(app(), [], [group()], {});
    expect(resolved.blockReason).toBe("tunnel_deleted");
    expect(resolved.phase).toBe("error");
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

  it("passes the live endpoint information through", () => {
    const live = status({
      endpoint: { endpointId: "e2", label: "frp 外网", endpoint: "ops@frp:22" },
    });
    const resolved = resolveAppStatus(app(), [tunnel()], [group()], { a1: live });
    expect(resolved.endpoint?.label).toBe("frp 外网");
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

  it("is true when any line is dialable", () => {
    expect(
      tunnelHasDialableEndpoint(
        tunnel({
          endpoints: [
            endpoint({ id: "off", dialable: false, enabled: false }),
            endpoint({ id: "ok" }),
          ],
        }),
      ),
    ).toBe(true);
  });

  it("is false when no line can be dialed", () => {
    expect(
      tunnelHasDialableEndpoint(tunnel({ endpoints: [endpoint({ dialable: false })] })),
    ).toBe(false);
    expect(tunnelHasDialableEndpoint(tunnel({ endpoints: [] }))).toBe(false);
  });
});

describe("usedFallback", () => {
  const twoLines = tunnel({
    endpoints: [endpoint({ id: "home" }), endpoint({ id: "away" })],
    preferredEndpointId: "home",
  });

  it("is false without failures", () => {
    expect(usedFallback(status(), twoLines)).toBe(false);
  });

  it("is false when the preferred line is in use", () => {
    const live = status({
      endpoint: { endpointId: "home", label: "家庭", endpoint: "a" },
      endpointFailures: [{ endpointId: "other", label: "x", code: "timeout", detail: null }],
    });
    expect(usedFallback(live, twoLines)).toBe(false);
  });

  it("is true when a fallback line carries the traffic", () => {
    const live = status({
      endpoint: { endpointId: "away", label: "frp 外网", endpoint: "b" },
      endpointFailures: [
        { endpointId: "home", label: "家庭", code: "timeout", detail: null },
      ],
    });
    expect(usedFallback(live, twoLines)).toBe(true);
  });

  it("is false before the connection is up", () => {
    const reconnecting = status({
      phase: "reconnecting",
      endpoint: { endpointId: "away", label: "frp 外网", endpoint: "b" },
      endpointFailures: [
        { endpointId: "home", label: "家庭", code: "timeout", detail: null },
      ],
    });
    expect(usedFallback(reconnecting, twoLines)).toBe(false);
  });

  it("is false when the user pinned nothing (pure auto order)", () => {
    const auto = tunnel({
      endpoints: [endpoint({ id: "home" }), endpoint({ id: "away" })],
      preferredEndpointId: null,
    });
    const live = status({
      endpoint: { endpointId: "away", label: "frp 外网", endpoint: "b" },
      endpointFailures: [
        { endpointId: "home", label: "家庭", code: "timeout", detail: null },
      ],
    });
    expect(usedFallback(live, auto)).toBe(false);
  });
});

describe("preferredEndpoint", () => {
  it("returns the pinned line", () => {
    const tunnelView = tunnel({
      endpoints: [endpoint({ id: "home" }), endpoint({ id: "away" })],
      preferredEndpointId: "away",
    });
    expect(preferredEndpoint(tunnelView)?.id).toBe("away");
  });

  it("falls back to the first line when the pin is stale", () => {
    const tunnelView = tunnel({
      endpoints: [endpoint({ id: "home" })],
      preferredEndpointId: "gone",
    });
    expect(preferredEndpoint(tunnelView)?.id).toBe("home");
  });

  it("is undefined without a tunnel", () => {
    expect(preferredEndpoint(undefined)).toBeUndefined();
  });
});
