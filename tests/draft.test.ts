import { describe, expect, it } from "vitest";
import type {
  AppView,
  EndpointView,
  GroupView,
  JumpInput,
  TunnelView,
} from "../src/api";
import {
  MAX_ENDPOINTS,
  MAX_JUMPS,
  appFormFrom,
  chainPreview,
  duplicateEndpointLabel,
  endpointChainPreview,
  endpointError,
  endpointFormFrom,
  hasSavedPassword,
  jumpError,
  jumpFormFrom,
  nextEndpointLabel,
  nextPort,
  toAppInput,
  toTunnelInput,
  tunnelFormFrom,
  usesLocalKey,
  validateApp,
  validateTunnel,
  type AppForm,
  type EndpointForm,
  type JumpForm,
  type TunnelForm,
} from "../src/draft";
import { strings } from "../src/strings";

function endpointForm(overrides: Partial<EndpointForm> = {}): EndpointForm {
  return {
    ...endpointFormFrom(undefined),
    label: "外网",
    host: "example.com",
    port: "22",
    username: "ops",
    authType: "password",
    passwordInput: "s3cret",
    ...overrides,
  };
}

function tunnelForm(overrides: Partial<TunnelForm> = {}): TunnelForm {
  return {
    ...tunnelFormFrom(undefined),
    name: "家庭 NAS",
    endpoints: [endpointForm()],
    ...overrides,
  };
}

function appForm(overrides: Partial<AppForm> = {}): AppForm {
  return {
    ...appFormFrom(undefined),
    name: "公司后台",
    groupId: "default",
    tunnelId: "t1",
    localPort: "18080",
    remoteHost: "127.0.0.1",
    remotePort: "5000",
    ...overrides,
  };
}

function endpointView(overrides: Partial<EndpointView> = {}): EndpointView {
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

function tunnelView(overrides: Partial<TunnelView> = {}): TunnelView {
  return {
    id: "t1",
    name: "NAS",
    endpoints: [endpointView()],
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

function appView(overrides: Partial<AppView> = {}): AppView {
  return {
    id: "a1",
    name: "Web",
    groupId: "default",
    tunnelId: "t1",
    localPort: 18080,
    remoteHost: "127.0.0.1",
    remotePort: 5000,
    browserUrl: "",
    browserOpenUrl: "http://127.0.0.1:18080/",
    localUrl: "http://127.0.0.1:18080/",
    autoStartOnBoot: false,
    desiredOn: false,
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

function jump(overrides: Partial<JumpForm> = {}): JumpForm {
  return {
    ...jumpFormFrom(),
    host: "bastion",
    username: "ops",
    passwordInput: "s3cret",
    ...overrides,
  };
}

describe("nextPort", () => {
  it("starts at 18080 when nothing is used", () => {
    expect(nextPort([])).toBe(18080);
  });

  it("skips ports already taken by other applications", () => {
    const apps = [
      appView({ id: "a", localPort: 18080 }),
      appView({ id: "b", localPort: 18081 }),
      appView({ id: "c", localPort: 18083 }),
    ];
    expect(nextPort(apps)).toBe(18082);
  });
});

describe("nextEndpointLabel", () => {
  it("starts at 线路 1 for a tunnel with no lines", () => {
    expect(nextEndpointLabel([])).toBe(strings.endpointDefaultLabel(1));
  });

  it("skips labels that are already taken", () => {
    const first = strings.endpointDefaultLabel(1);
    const second = strings.endpointDefaultLabel(2);
    expect(nextEndpointLabel([first])).toBe(second);
    expect(nextEndpointLabel([first, second])).toBe(
      strings.endpointDefaultLabel(3),
    );
  });
});

describe("validateTunnel", () => {
  it("accepts a complete form", () => {
    expect(validateTunnel(tunnelForm())).toEqual({});
  });

  it("requires a name", () => {
    expect(validateTunnel(tunnelForm({ name: " " })).name).toBe(
      strings.errorNameRequired,
    );
  });

  it("rejects names longer than 40 characters", () => {
    expect(validateTunnel(tunnelForm({ name: "名".repeat(41) })).name).toBe(
      strings.errorNameTooLong,
    );
  });

  it("counts unicode characters, not bytes, for the name limit", () => {
    expect(
      validateTunnel(tunnelForm({ name: "名".repeat(40) })).name,
    ).toBeUndefined();
  });

  it("enforces keepalive and timeout bounds", () => {
    expect(
      validateTunnel(tunnelForm({ keepAliveSeconds: "4" })).keepAliveSeconds,
    ).toBe(strings.errorKeepalive);
    expect(
      validateTunnel(tunnelForm({ keepAliveSeconds: "301" })).keepAliveSeconds,
    ).toBe(strings.errorKeepalive);
    expect(
      validateTunnel(tunnelForm({ connectTimeoutSeconds: "121" }))
        .connectTimeoutSeconds,
    ).toBe(strings.errorTimeout);
    expect(
      validateTunnel(
        tunnelForm({ keepAliveSeconds: "5", connectTimeoutSeconds: "120" }),
      ),
    ).toEqual({});
  });

  it("requires at least one endpoint", () => {
    expect(validateTunnel(tunnelForm({ endpoints: [] })).endpoints).toBe(
      strings.errorEndpointRequired,
    );
  });

  it("rejects a tunnel whose lines are all disabled", () => {
    expect(
      validateTunnel(
        tunnelForm({ endpoints: [endpointForm({ enabled: false })] }),
      ).endpoints,
    ).toBe(strings.errorEndpointAllDisabled);
  });

  it("rejects more than the endpoint limit", () => {
    const endpoints = Array.from({ length: MAX_ENDPOINTS + 1 }, (_, index) =>
      endpointForm({ label: `线路 ${index + 1}` }),
    );
    expect(validateTunnel(tunnelForm({ endpoints })).endpoints).toBe(
      strings.errorMaxEndpoints,
    );
  });

  it("surfaces the first incomplete line", () => {
    expect(
      validateTunnel(
        tunnelForm({
          endpoints: [
            endpointForm(),
            endpointForm({ label: "内网", host: "bad host" }),
          ],
        }),
      ).endpoints,
    ).toBe(strings.errorEndpointHostRequired);
  });

  it("rejects duplicate line names", () => {
    expect(
      validateTunnel(
        tunnelForm({
          endpoints: [
            endpointForm(),
            endpointForm({ label: "外网", host: "other.example.com" }),
          ],
        }),
      ).endpoints,
    ).toBe(strings.errorEndpointLabelDuplicate);
  });
});

describe("endpointError", () => {
  it("accepts a complete line", () => {
    expect(endpointError(endpointForm())).toBeNull();
  });

  it("validates host, port and username", () => {
    expect(endpointError(endpointForm({ host: " " }))).toBe(
      strings.errorEndpointHostRequired,
    );
    expect(endpointError(endpointForm({ host: "bad host" }))).toBe(
      strings.errorEndpointHostRequired,
    );
    expect(endpointError(endpointForm({ port: "0" }))).toBe(
      strings.errorEndpointPort,
    );
    expect(endpointError(endpointForm({ port: "abc" }))).toBe(
      strings.errorEndpointPort,
    );
    expect(endpointError(endpointForm({ username: "" }))).toBe(
      strings.errorEndpointUsernameRequired,
    );
  });

  it("bounds the line label", () => {
    expect(endpointError(endpointForm({ label: "名".repeat(41) }))).toBe(
      strings.errorEndpointLabelTooLong,
    );
  });

  it("requires a secret only when none is saved", () => {
    expect(
      endpointError(endpointForm({ passwordInput: "", hasPassword: false })),
    ).toBe(strings.errorEndpointPasswordRequired);
    expect(
      endpointError(endpointForm({ passwordInput: "", hasPassword: true })),
    ).toBeNull();
    expect(
      endpointError(
        endpointForm({
          passwordInput: "",
          hasPassword: true,
          passwordCleared: true,
        }),
      ),
    ).toBe(strings.errorEndpointPasswordRequired);
  });

  it("requires a private key when key auth is selected", () => {
    expect(
      endpointError(endpointForm({ authType: "private_key", privateKeyInput: "" })),
    ).toBe(strings.errorEndpointPrivateKeyRequired);
  });

  it("accepts a locally selected ~/.ssh key without pasting its content", () => {
    expect(
      endpointError(
        endpointForm({
          authType: "private_key",
          privateKeyInput: "",
          privateKeyPath: "id_ed25519",
        }),
      ),
    ).toBeNull();
  });

  it("rejects more than the jump limit", () => {
    const jumps = Array.from({ length: MAX_JUMPS + 1 }, () => jump());
    expect(endpointError(endpointForm({ jumps }))).toBe(strings.errorMaxJumps);
  });

  it("rejects an incomplete jump inside the line", () => {
    expect(endpointError(endpointForm({ jumps: [jump({ host: "" })] }))).toBe(
      strings.errorJumpInvalid,
    );
  });
});

describe("duplicateEndpointLabel", () => {
  it("ignores unnamed lines and itself", () => {
    const endpoints = [
      endpointForm({ label: "" }),
      endpointForm({ label: "", host: "b.example.com" }),
    ];
    expect(duplicateEndpointLabel(endpoints, 0)).toBe(false);
    expect(duplicateEndpointLabel(endpoints, 1)).toBe(false);
  });

  it("detects a repeated name", () => {
    const endpoints = [
      endpointForm({ label: "外网" }),
      endpointForm({ label: "外网", host: "b.example.com" }),
    ];
    expect(duplicateEndpointLabel(endpoints, 0)).toBe(true);
  });
});

describe("validateApp", () => {
  const groups = [group({ id: "default", isDefault: true })];
  const tunnels = [tunnelView()];

  it("accepts a complete form", () => {
    expect(validateApp(appForm(), groups, tunnels)).toEqual({});
  });

  it("requires a name, group and tunnel", () => {
    const errors = validateApp(
      appForm({ name: " ", groupId: "gone", tunnelId: "gone" }),
      groups,
      tunnels,
    );
    expect(errors.name).toBe(strings.errorAppNameRequired);
    expect(errors.groupId).toBe(strings.errorGroupRequired);
    expect(errors.tunnelId).toBe(strings.errorTunnelRequired);
  });

  it("rejects privileged and out-of-range ports", () => {
    expect(validateApp(appForm({ localPort: "80" }), groups, tunnels).localPort).toBe(
      strings.errorLocalPort,
    );
    expect(
      validateApp(appForm({ localPort: "abc" }), groups, tunnels).localPort,
    ).toBe(strings.errorLocalPort);
    expect(
      validateApp(appForm({ remotePort: "70000" }), groups, tunnels).remotePort,
    ).toBe(strings.errorRemotePort);
  });

  it("rejects hostnames containing whitespace", () => {
    expect(
      validateApp(appForm({ remoteHost: "10.0.0.1 x" }), groups, tunnels)
        .remoteHost,
    ).toBe(strings.errorRemoteHostRequired);
  });

  it("checks the browser URL scheme case-insensitively", () => {
    expect(
      validateApp(
        appForm({ browserUrl: "HTTPS://127.0.0.1:{port}/" }),
        groups,
        tunnels,
      ),
    ).toEqual({});
    expect(
      validateApp(
        appForm({ browserUrl: "ftp://127.0.0.1/" }),
        groups,
        tunnels,
      ).browserUrl,
    ).toBe(strings.errorBrowserUrl);
  });

  it("rejects names longer than 40 characters", () => {
    expect(
      validateApp(appForm({ name: "名".repeat(41) }), groups, tunnels).name,
    ).toBe(strings.errorAppNameTooLong);
  });
});

describe("jumpError", () => {
  it("accepts a complete password jump", () => {
    expect(jumpError(jump())).toBeNull();
  });

  it("validates host, port, username and secret", () => {
    expect(jumpError(jump({ host: " " }))).toBe(strings.errorJumpHostRequired);
    expect(jumpError(jump({ port: "0" }))).toBe(strings.errorJumpPort);
    expect(jumpError(jump({ username: "" }))).toBe(
      strings.errorJumpUsernameRequired,
    );
    expect(jumpError(jump({ passwordInput: "" }))).toBe(
      strings.errorJumpPasswordRequired,
    );
  });

  it("accepts a saved secret that was not re-entered", () => {
    expect(jumpError(jump({ passwordInput: "", hasPassword: true }))).toBeNull();
  });

  it("requires a private key for key auth", () => {
    expect(
      jumpError(jump({ authType: "private_key", privateKeyInput: "" })),
    ).toBe(strings.errorJumpPrivateKeyRequired);
  });
});

describe("toTunnelInput", () => {
  it("sends null for untouched secrets so the backend keeps them", () => {
    const input = toTunnelInput(
      tunnelForm({
        id: "t1",
        endpoints: [endpointForm({ id: "e1", hasPassword: true, passwordInput: "" })],
      }),
    );
    expect(input.endpoints[0].credentials.password).toBeNull();
    expect(input.endpoints[0].credentials.privateKey).toBeNull();
    expect(input.endpoints[0].credentials.passphrase).toBeNull();
  });

  it("sends an empty string when the user cleared a stored secret", () => {
    const input = toTunnelInput(
      tunnelForm({
        id: "t1",
        endpoints: [
          endpointForm({ id: "e1", hasPassword: true, passwordCleared: true }),
        ],
      }),
    );
    expect(input.endpoints[0].credentials.password).toBe("");
  });

  it("sends the new secret when the user typed one", () => {
    expect(
      toTunnelInput(tunnelForm({ endpoints: [endpointForm({ passwordInput: "new-secret" })] }))
        .endpoints[0].credentials.password,
    ).toBe("new-secret");
  });

  it("keeps every line, its order and its switches", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [
          endpointForm({
            id: "home",
            label: "家庭局域网",
            host: "192.168.1.10",
            wifiOnly: true,
          }),
          endpointForm({ id: "away", label: "frp 外网", host: "frp.example.com" }),
        ],
        preferredEndpointId: "away",
      }),
    );
    expect(input.endpoints).toHaveLength(2);
    expect(input.endpoints[0].label).toBe("家庭局域网");
    expect(input.endpoints[0].wifiOnly).toBe(true);
    expect(input.endpoints[1].label).toBe("frp 外网");
    expect(input.preferredEndpointId).toBe("away");
  });

  it("drops a preferred line that no longer exists", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [endpointForm({ id: "home" })],
        preferredEndpointId: "deleted",
      }),
    );
    expect(input.preferredEndpointId).toBeNull();
  });

  it("parallelizes jump credentials per line", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [
          endpointForm({
            jumps: [
              jump({ passwordInput: "", hasPassword: true }),
              jump({ passwordInput: "typed" }),
            ],
          }),
        ],
      }),
    );
    expect(input.endpoints[0].jumps[0].credentials.password).toBeNull();
    expect(input.endpoints[0].jumps[1].credentials.password).toBe("typed");
  });

  it("trims text fields", () => {
    const input = toTunnelInput(
      tunnelForm({
        name: "  NAS  ",
        endpoints: [
          endpointForm({ label: " 外网 ", host: " example.com ", username: " ops " }),
        ],
      }),
    );
    expect(input.name).toBe("NAS");
    expect(input.endpoints[0].label).toBe("外网");
    expect(input.endpoints[0].host).toBe("example.com");
    expect(input.endpoints[0].username).toBe("ops");
  });

  it("converts numeric strings to numbers", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [endpointForm({ port: "2222" })],
        keepAliveSeconds: "45",
      }),
    );
    expect(input.endpoints[0].port).toBe(2222);
    expect(input.keepAliveSeconds).toBe(45);
  });

  it("keeps the jump order stable when serializing", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [
          endpointForm({
            jumps: [jump({ label: "first" }), jump({ label: "second" })],
          }),
        ],
      }),
    );
    expect(input.endpoints[0].jumps.map((item: JumpInput) => item.label)).toEqual([
      "first",
      "second",
    ]);
  });

  it("passes only the ~/.ssh file name, never its content", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [
          endpointForm({
            authType: "private_key",
            privateKeyInput: "",
            privateKeyPath: "  id_ed25519  ",
            jumps: [
              jump({
                authType: "private_key",
                passwordInput: "",
                privateKeyPath: "id_rsa",
              }),
            ],
          }),
        ],
      }),
    );
    expect(input.endpoints[0].credentials.privateKeyPath).toBe("id_ed25519");
    // 仍然发 null，后端以路径为准读取文件，避免把旧内容误当成新密钥。
    expect(input.endpoints[0].credentials.privateKey).toBeNull();
    expect(input.endpoints[0].jumps[0].credentials.privateKeyPath).toBe("id_rsa");
  });

  it("does not send a stale local key path after switching to password auth", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [endpointForm({ authType: "password", privateKeyPath: "id_rsa" })],
      }),
    );
    expect(input.endpoints[0].credentials.privateKeyPath).toBeNull();
  });

  it("omits the local key reference when the user pasted content", () => {
    const input = toTunnelInput(
      tunnelForm({
        endpoints: [
          endpointForm({
            authType: "private_key",
            privateKeyInput: "-----BEGIN KEY-----",
          }),
        ],
      }),
    );
    expect(input.endpoints[0].credentials.privateKeyPath).toBeNull();
    expect(input.endpoints[0].credentials.privateKey).toBe("-----BEGIN KEY-----");
  });
});

describe("toAppInput", () => {
  it("trims text fields and converts ports", () => {
    const input = toAppInput(
      appForm({
        name: "  后台  ",
        localPort: "18090",
        remoteHost: " 10.0.0.5 ",
        remotePort: "5000",
        browserUrl: " http://127.0.0.1:{port}/ ",
      }),
    );
    expect(input.name).toBe("后台");
    expect(input.localPort).toBe(18090);
    expect(input.remoteHost).toBe("10.0.0.5");
    expect(input.remotePort).toBe(5000);
    expect(input.browserUrl).toBe("http://127.0.0.1:{port}/");
  });

  it("preserves the group, tunnel and boot intent", () => {
    const input = toAppInput(
      appForm({ groupId: "g1", tunnelId: "t2", autoStartOnBoot: true }),
    );
    expect(input.groupId).toBe("g1");
    expect(input.tunnelId).toBe("t2");
    expect(input.autoStartOnBoot).toBe(true);
  });
});

describe("form hydration", () => {
  it("never exposes stored tunnel secrets into the form", () => {
    const restored = tunnelFormFrom(tunnelView());
    expect(restored.endpoints[0].hasPassword).toBe(true);
    expect(restored.endpoints[0].passwordInput).toBe("");
    expect(restored.endpoints[0].passwordCleared).toBe(false);
    expect(hasSavedPassword(restored.endpoints[0])).toBe(true);
  });

  it("starts with no ~/.ssh key selected", () => {
    expect(tunnelFormFrom(undefined).endpoints).toEqual([]);
    expect(endpointFormFrom(undefined).privateKeyPath).toBeNull();
    expect(jumpFormFrom(undefined).privateKeyPath).toBeNull();
    expect(usesLocalKey(endpointFormFrom(undefined))).toBe(false);
  });

  it("round-trips multi-line tunnels including switches and preference", () => {
    const source = tunnelView({
      endpoints: [
        endpointView({
          id: "home",
          label: "家庭局域网",
          host: "192.168.1.10",
          wifiOnly: true,
        }),
        endpointView({
          id: "away",
          label: "frp 外网",
          host: "frp.example.com",
          port: 2222,
          enabled: false,
          jumps: [
            {
              id: "jump-1",
              label: "堡垒机",
              host: "bastion",
              port: 2222,
              username: "ops",
              credentials: {
                authType: "private_key",
                hasPassword: false,
                hasPrivateKey: true,
                hasPassphrase: true,
              },
              inheritedFrom: null,
            },
          ],
        }),
      ],
      preferredEndpointId: "away",
    });
    const restored = tunnelFormFrom(source);
    expect(restored.endpoints).toHaveLength(2);
    expect(restored.preferredEndpointId).toBe("away");
    expect(restored.endpoints[0].wifiOnly).toBe(true);
    expect(restored.endpoints[1].enabled).toBe(false);
    expect(restored.endpoints[1].port).toBe("2222");
    expect(restored.endpoints[1].jumps).toHaveLength(1);
    expect(restored.endpoints[1].jumps[0].authType).toBe("private_key");
    expect(restored.endpoints[1].jumps[0].hasPrivateKey).toBe(true);
    expect(restored.endpoints[1].jumps[0].hasPassphrase).toBe(true);
    expect(restored.endpoints[1].jumps[0].privateKeyInput).toBe("");
  });

  it("uses editor defaults for a new application", () => {
    const form = appFormFrom(undefined, {
      groupId: "g1",
      tunnelId: "t2",
      localPort: 18090,
    });
    expect(form.groupId).toBe("g1");
    expect(form.tunnelId).toBe("t2");
    expect(form.localPort).toBe("18090");
  });

  it("round-trips an existing application", () => {
    const restored = appFormFrom(
      appView({ groupId: "g1", tunnelId: "t2", autoStartOnBoot: true }),
    );
    expect(restored.id).toBe("a1");
    expect(restored.groupId).toBe("g1");
    expect(restored.tunnelId).toBe("t2");
    expect(restored.localPort).toBe("18080");
    expect(restored.autoStartOnBoot).toBe(true);
  });
});

describe("chainPreview", () => {
  it("falls back to the placeholder when there is no line", () => {
    expect(chainPreview(tunnelFormFrom(undefined))).toBe(strings.editorNoEndpoint);
  });

  it("lists jumps in order before the target", () => {
    const preview = chainPreview(
      tunnelForm({
        endpoints: [
          endpointForm({
            host: "server",
            jumps: [jump({ label: "A", host: "a" }), jump({ label: "", host: "b" })],
          }),
        ],
      }),
    );
    expect(preview).toBe("A  →  b  →  server");
  });

  it("describes the preferred line, not just the first one", () => {
    const preview = chainPreview(
      tunnelForm({
        endpoints: [
          endpointForm({ id: "home", host: "192.168.1.10" }),
          endpointForm({ id: "away", host: "frp.example.com" }),
        ],
        preferredEndpointId: "away",
      }),
    );
    expect(preview).toContain("frp.example.com");
    expect(preview).toContain(strings.endpointCountSuffix(2));
  });

  it("appends the line count only when there are several", () => {
    expect(
      chainPreview(tunnelForm({ endpoints: [endpointForm({ host: "a" })] })),
    ).toBe("a");
  });
});

describe("endpointChainPreview", () => {
  it("shows the placeholder for an empty address", () => {
    expect(endpointChainPreview(endpointForm({ host: "", jumps: [] }))).toBe(
      strings.editorServerPlaceholder,
    );
  });

  it("joins jumps with the target", () => {
    expect(
      endpointChainPreview(
        endpointForm({ host: "target", jumps: [jump({ label: "", host: "b1" })] }),
      ),
    ).toBe("b1  →  target");
  });
});
