import { describe, expect, it } from "vitest";
import type {
  AppView,
  GroupView,
  JumpInput,
  TunnelView,
} from "../src/api";
import {
  MAX_APP_TUNNELS,
  MAX_JUMPS,
  appFormFrom,
  appHasDialableTunnel,
  appTunnels,
  chainPreview,
  hasSavedPassword,
  jumpError,
  jumpFormFrom,
  jumpFormsFromTunnel,
  jumpFromTunnel,
  jumpLevelsAdded,
  nextPort,
  selectableJumpTunnels,
  toAppInput,
  toTunnelInput,
  tunnelChainPreview,
  tunnelError,
  tunnelFormFrom,
  usesLocalKey,
  validateApp,
  validateTunnel,
  type AppForm,
  type JumpForm,
  type TunnelForm,
} from "../src/draft";
import { resolveAppStatus } from "../src/status";
import { strings } from "../src/strings";

function tunnelForm(overrides: Partial<TunnelForm> = {}): TunnelForm {
  return {
    ...tunnelFormFrom(undefined),
    name: "外网入口",
    host: "example.com",
    port: "22",
    username: "ops",
    authType: "password",
    passwordInput: "s3cret",
    ...overrides,
  };
}

function appForm(overrides: Partial<AppForm> = {}): AppForm {
  return {
    ...appFormFrom(undefined),
    name: "公司后台",
    groupId: "default",
    tunnelIds: ["t1"],
    localPort: "18080",
    remoteHost: "127.0.0.1",
    remotePort: "5000",
    ...overrides,
  };
}

function tunnelView(overrides: Partial<TunnelView> = {}): TunnelView {
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

function appView(overrides: Partial<AppView> = {}): AppView {
  return {
    id: "a1",
    name: "Web",
    groupId: "default",
    tunnelIds: ["t1"],
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
    expect(validateTunnel(tunnelForm({ name: "名".repeat(40) })).name).toBe(
      undefined,
    );
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

  it("surfaces an incomplete access path under the access section", () => {
    expect(validateTunnel(tunnelForm({ host: "bad host" })).access).toBe(
      strings.errorTunnelHostRequired,
    );
  });
});

describe("tunnelError", () => {
  it("accepts a complete access path", () => {
    expect(tunnelError(tunnelForm())).toBeNull();
  });

  it("validates host, port and username", () => {
    expect(tunnelError(tunnelForm({ host: " " }))).toBe(
      strings.errorTunnelHostRequired,
    );
    expect(tunnelError(tunnelForm({ host: "bad host" }))).toBe(
      strings.errorTunnelHostRequired,
    );
    expect(tunnelError(tunnelForm({ port: "0" }))).toBe(
      strings.errorTunnelPort,
    );
    expect(tunnelError(tunnelForm({ port: "abc" }))).toBe(
      strings.errorTunnelPort,
    );
    expect(tunnelError(tunnelForm({ username: "" }))).toBe(
      strings.errorTunnelUsernameRequired,
    );
  });

  it("requires a secret only when none is saved", () => {
    expect(
      tunnelError(tunnelForm({ passwordInput: "", hasPassword: false })),
    ).toBe(strings.errorTunnelPasswordRequired);
    expect(
      tunnelError(tunnelForm({ passwordInput: "", hasPassword: true })),
    ).toBeNull();
    expect(
      tunnelError(
        tunnelForm({
          passwordInput: "",
          hasPassword: true,
          passwordCleared: true,
        }),
      ),
    ).toBe(strings.errorTunnelPasswordRequired);
  });

  it("requires a private key when key auth is selected", () => {
    expect(
      tunnelError(tunnelForm({ authType: "private_key", privateKeyInput: "" })),
    ).toBe(strings.errorTunnelPrivateKeyRequired);
  });

  it("accepts a locally selected ~/.ssh key without pasting its content", () => {
    expect(
      tunnelError(
        tunnelForm({
          authType: "private_key",
          privateKeyInput: "",
          privateKeyPath: "id_ed25519",
        }),
      ),
    ).toBeNull();
  });

  it("rejects more than the jump limit", () => {
    const jumps = Array.from({ length: MAX_JUMPS + 1 }, () => jump());
    expect(tunnelError(tunnelForm({ jumps }))).toBe(strings.errorMaxJumps);
  });

  it("rejects an incomplete jump inside the tunnel", () => {
    expect(tunnelError(tunnelForm({ jumps: [jump({ host: "" })] }))).toBe(
      strings.errorJumpInvalid,
    );
  });
});

describe("validateApp", () => {
  const groups = [group({ id: "default", isDefault: true })];
  const tunnels = [tunnelView()];

  it("accepts a complete form", () => {
    expect(validateApp(appForm(), groups, tunnels)).toEqual({});
  });

  it("requires a name, group and at least one tunnel", () => {
    const errors = validateApp(
      appForm({ name: " ", groupId: "gone", tunnelIds: [] }),
      groups,
      tunnels,
    );
    expect(errors.name).toBe(strings.errorAppNameRequired);
    expect(errors.groupId).toBe(strings.errorGroupRequired);
    expect(errors.tunnelIds).toBe(strings.errorTunnelRequired);
  });

  it("rejects a tunnel that no longer exists", () => {
    expect(
      validateApp(appForm({ tunnelIds: ["gone"] }), groups, tunnels).tunnelIds,
    ).toBe(strings.errorAppTunnelMissing);
  });

  it("rejects a selection where no tunnel is dialable", () => {
    expect(
      validateApp(appForm(), groups, [tunnelView({ dialable: false })])
        .tunnelIds,
    ).toBe(strings.errorNoDialableTunnel);
  });

  it("accepts a selection where at least one tunnel is dialable", () => {
    expect(
      validateApp(
        appForm({ tunnelIds: ["off", "ok"] }),
        groups,
        [tunnelView({ id: "off", dialable: false }), tunnelView({ id: "ok" })],
      ),
    ).toEqual({});
  });

  it("rejects more tunnels than the limit", () => {
    const many = Array.from({ length: MAX_APP_TUNNELS + 1 }, (_, index) =>
      tunnelView({ id: `t${index}` }),
    );
    expect(
      validateApp(
        appForm({ tunnelIds: many.map((tunnel) => tunnel.id) }),
        groups,
        many,
      ).tunnelIds,
    ).toBe(strings.errorMaxAppTunnels);
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
      tunnelForm({ id: "t1", hasPassword: true, passwordInput: "" }),
    );
    expect(input.credentials.password).toBeNull();
    expect(input.credentials.privateKey).toBeNull();
    expect(input.credentials.passphrase).toBeNull();
  });

  it("sends an empty string when the user cleared a stored secret", () => {
    const input = toTunnelInput(
      tunnelForm({ id: "t1", hasPassword: true, passwordCleared: true }),
    );
    expect(input.credentials.password).toBe("");
  });

  it("sends the new secret when the user typed one", () => {
    expect(
      toTunnelInput(tunnelForm({ passwordInput: "new-secret" })).credentials
        .password,
    ).toBe("new-secret");
  });

  it("trims text fields and converts ports", () => {
    const input = toTunnelInput(
      tunnelForm({
        name: "  外网  ",
        host: " example.com ",
        username: " ops ",
        port: "2222",
        keepAliveSeconds: "45",
      }),
    );
    expect(input.name).toBe("外网");
    expect(input.host).toBe("example.com");
    expect(input.username).toBe("ops");
    expect(input.port).toBe(2222);
    expect(input.keepAliveSeconds).toBe(45);
  });

  it("keeps the jump order stable when serializing", () => {
    const input = toTunnelInput(
      tunnelForm({
        jumps: [jump({ label: "first" }), jump({ label: "second" })],
      }),
    );
    expect(input.jumps.map((item: JumpInput) => item.label)).toEqual([
      "first",
      "second",
    ]);
  });

  it("passes only the ~/.ssh file name, never its content", () => {
    const input = toTunnelInput(
      tunnelForm({
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
    );
    expect(input.credentials.privateKeyPath).toBe("id_ed25519");
    // 仍然发 null，后端以路径为准读取文件，避免把旧内容误当成新密钥。
    expect(input.credentials.privateKey).toBeNull();
    expect(input.jumps[0].credentials.privateKeyPath).toBe("id_rsa");
  });

  it("does not send a stale local key path after switching to password auth", () => {
    const input = toTunnelInput(
      tunnelForm({ authType: "password", privateKeyPath: "id_rsa" }),
    );
    expect(input.credentials.privateKeyPath).toBeNull();
  });

  it("omits the local key reference when the user pasted content", () => {
    const input = toTunnelInput(
      tunnelForm({
        authType: "private_key",
        privateKeyInput: "-----BEGIN KEY-----",
      }),
    );
    expect(input.credentials.privateKeyPath).toBeNull();
    expect(input.credentials.privateKey).toBe("-----BEGIN KEY-----");
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

  it("keeps the group, tunnel order and boot intent", () => {
    const input = toAppInput(
      appForm({
        groupId: "g1",
        tunnelIds: ["away", "home"],
        autoStartOnBoot: true,
      }),
    );
    expect(input.groupId).toBe("g1");
    expect(input.tunnelIds).toEqual(["away", "home"]);
    expect(input.autoStartOnBoot).toBe(true);
  });

  it("collapses duplicate tunnel choices without reordering", () => {
    const input = toAppInput(
      appForm({ tunnelIds: ["away", "home", "away"] }),
    );
    expect(input.tunnelIds).toEqual(["away", "home"]);
  });
});

describe("appTunnels", () => {
  const tunnels = [
    tunnelView({ id: "home", name: "家里" }),
    tunnelView({ id: "away", name: "外网" }),
  ];

  it("returns the declared order and skips deleted tunnels", () => {
    expect(
      appTunnels({ tunnelIds: ["away", "gone", "home"] }, tunnels).map(
        (tunnel) => tunnel.id,
      ),
    ).toEqual(["away", "home"]);
  });

  it("reports whether anything can be dialled", () => {
    expect(appHasDialableTunnel({ tunnelIds: ["home"] }, tunnels)).toBe(true);
    expect(
      appHasDialableTunnel({ tunnelIds: ["home"] }, [
        tunnelView({ id: "home", dialable: false }),
      ]),
    ).toBe(false);
    expect(appHasDialableTunnel({ tunnelIds: [] }, tunnels)).toBe(false);
  });
});

describe("form hydration", () => {
  it("never exposes stored tunnel secrets into the form", () => {
    const restored = tunnelFormFrom(tunnelView());
    expect(restored.hasPassword).toBe(true);
    expect(restored.passwordInput).toBe("");
    expect(restored.passwordCleared).toBe(false);
    expect(hasSavedPassword(restored)).toBe(true);
  });

  it("starts with an empty access path and no ~/.ssh key selected", () => {
    const empty = tunnelFormFrom(undefined);
    expect(empty.host).toBe("");
    expect(empty.jumps).toEqual([]);
    expect(empty.privateKeyPath).toBeNull();
    expect(jumpFormFrom(undefined).privateKeyPath).toBeNull();
    expect(usesLocalKey(tunnelFormFrom(undefined))).toBe(false);
  });

  it("round-trips a tunnel including its jump chain and switches", () => {
    const source = tunnelView({
      id: "away",
      name: "外网",
      host: "frp.example.com",
      port: 2222,
      autoReconnect: false,
      strictHostKey: false,
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
    });
    const restored = tunnelFormFrom(source);
    expect(restored.name).toBe("外网");
    expect(restored.host).toBe("frp.example.com");
    expect(restored.port).toBe("2222");
    expect(restored.autoReconnect).toBe(false);
    expect(restored.strictHostKey).toBe(false);
    expect(restored.jumps).toHaveLength(1);
    expect(restored.jumps[0].authType).toBe("private_key");
    expect(restored.jumps[0].hasPrivateKey).toBe(true);
    expect(restored.jumps[0].hasPassphrase).toBe(true);
    expect(restored.jumps[0].privateKeyInput).toBe("");
  });

  it("uses editor defaults for a new application", () => {
    const form = appFormFrom(undefined, {
      groupId: "g1",
      tunnelIds: ["t2"],
      localPort: 18090,
    });
    expect(form.groupId).toBe("g1");
    expect(form.tunnelIds).toEqual(["t2"]);
    expect(form.localPort).toBe("18090");
  });

  it("starts a new application with no tunnel chosen", () => {
    expect(appFormFrom(undefined).tunnelIds).toEqual([]);
  });

  it("round-trips an existing application and its tunnel order", () => {
    const restored = appFormFrom(
      appView({
        groupId: "g1",
        tunnelIds: ["away", "home"],
        autoStartOnBoot: true,
      }),
    );
    expect(restored.id).toBe("a1");
    expect(restored.groupId).toBe("g1");
    expect(restored.tunnelIds).toEqual(["away", "home"]);
    expect(restored.localPort).toBe("18080");
    expect(restored.autoStartOnBoot).toBe(true);
  });
});

describe("chainPreview", () => {
  it("falls back to the placeholder when nothing is configured", () => {
    expect(chainPreview(tunnelFormFrom(undefined))).toBe(
      strings.editorNoEndpoint,
    );
  });

  it("lists jumps in order before the target", () => {
    const preview = chainPreview(
      tunnelForm({
        host: "server",
        jumps: [jump({ label: "A", host: "a" }), jump({ label: "", host: "b" })],
      }),
    );
    expect(preview).toBe("A  →  b  →  server");
  });

  it("describes one access path only — a tunnel has no alternative lines", () => {
    const preview = chainPreview(
      tunnelForm({ host: "frp.example.com", jumps: [] }),
    );
    expect(preview).toBe("frp.example.com");
  });
});

describe("tunnelChainPreview", () => {
  it("shows the placeholder for an empty address", () => {
    expect(tunnelChainPreview(tunnelForm({ host: "", jumps: [] }))).toBe(
      strings.editorServerPlaceholder,
    );
  });

  it("joins jumps with the target", () => {
    expect(
      tunnelChainPreview(
        tunnelForm({ host: "target", jumps: [jump({ label: "", host: "b1" })] }),
      ),
    ).toBe("b1  →  target");
  });
});

describe("resolveAppStatus with tunnel lists", () => {
  const groups = [group()];

  it("keeps the app when one of several tunnels still exists", () => {
    const app = appView({ tunnelIds: ["gone", "t1"] });
    const resolved = resolveAppStatus(app, [tunnelView()], groups, {});
    expect(resolved.blockReason).toBe("none");
  });

  it("blocks the app when every referenced tunnel is gone", () => {
    const app = appView({ tunnelIds: ["gone"] });
    const resolved = resolveAppStatus(app, [tunnelView()], groups, {});
    expect(resolved.blockReason).toBe("tunnel_deleted");
  });

  it("blocks an app that references no tunnel at all", () => {
    expect(
      resolveAppStatus(appView({ tunnelIds: [] }), [tunnelView()], groups, {})
        .blockReason,
    ).toBe("tunnel_deleted");
  });
});

describe("picking a jump host from an existing tunnel", () => {
  it("copies address, user and auth from the tunnel", () => {
    const source = tunnelView({
      id: "bastion",
      name: "堡垒机",
      host: "bastion.example.com",
      port: 2222,
      username: "ops",
      credentials: {
        authType: "private_key",
        hasPassword: false,
        hasPrivateKey: true,
        hasPassphrase: true,
      },
    });
    const picked = jumpFormsFromTunnel(source);
    expect(picked).toHaveLength(1);
    expect(picked[0]).toMatchObject({
      id: null,
      label: "堡垒机",
      host: "bastion.example.com",
      port: "2222",
      username: "ops",
      authType: "private_key",
      hasPrivateKey: true,
      hasPassphrase: true,
      sourceTunnelId: "bastion",
      credentialFrom: { tunnelId: "bastion", hop: null },
    });
    // 不暴露任何密文/明文，只带“已配置”标记。
    expect(picked[0].passwordInput).toBe("");
    expect(picked[0].privateKeyInput).toBe("");
    expect(jumpFromTunnel(picked[0])).toBe(true);
    // 选过来的跳板本身就是完整的，可以直接保存。
    expect(jumpError(picked[0])).toBeNull();
  });

  it("keeps the tunnel's own jump chain in front of it", () => {
    const source = tunnelView({
      id: "vdi",
      name: "内网",
      host: "10.0.0.2",
      username: "root",
      jumps: [
        {
          id: "j1",
          label: "堡垒机",
          host: "10.0.0.1",
          port: 22,
          username: "root",
          credentials: {
            authType: "password",
            hasPassword: true,
            hasPrivateKey: false,
            hasPassphrase: false,
          },
          inheritedFrom: null,
        },
      ],
    });
    const picked = jumpFormsFromTunnel(source);
    expect(picked.map((item) => item.host)).toEqual(["10.0.0.1", "10.0.0.2"]);
    expect(picked[0].credentialFrom).toEqual({ tunnelId: "vdi", hop: 0 });
    expect(picked[1].credentialFrom).toEqual({ tunnelId: "vdi", hop: null });
    expect(jumpLevelsAdded(source)).toBe(2);
  });

  it("sends the credential source so the backend can copy the secret", () => {
    const source = tunnelView({ id: "bastion" });
    const input = toTunnelInput(
      tunnelForm({ host: "inner", jumps: jumpFormsFromTunnel(source) }),
    );
    expect(input.jumps[0].credentialFrom).toEqual({
      tunnelId: "bastion",
      hop: null,
    });
  });

  it("leaves a manually written jump without a credential source", () => {
    const input = toTunnelInput(tunnelForm({ host: "inner", jumps: [jump()] }));
    expect(input.jumps[0].credentialFrom).toBeNull();
  });

  it("inherited hop credentials stay inherited instead of being copied", () => {
    const source = tunnelView({
      jumps: [
        {
          id: "j1",
          label: "堡垒机",
          host: "10.0.0.1",
          port: 22,
          username: "root",
          // 已由另一条隧道解析出来的凭据：没有自己的密文。
          credentials: {
            authType: "password",
            hasPassword: true,
            hasPrivateKey: false,
            hasPassphrase: false,
          },
          inheritedFrom: "堡垒机",
        },
      ],
    });
    const picked = jumpFormsFromTunnel(source);
    expect(picked[0].credentialFrom).toBeNull();
    expect(picked[0].sourceTunnelId).toBe(source.id);
  });
});

describe("selectableJumpTunnels", () => {
  it("excludes the tunnel being edited and hosts already in the chain", () => {
    const editing = tunnelView({ id: "self", host: "self.example.com" });
    const already = tunnelView({ id: "used", host: "bastion", username: "ops" });
    const fresh = tunnelView({ id: "fresh", host: "other.example.com" });
    const jumps = [jump({ host: "bastion", username: "ops" })];
    const ids = selectableJumpTunnels(
      [editing, already, fresh],
      "self",
      jumps,
    ).map((tunnel) => tunnel.id);
    expect(ids).toEqual(["fresh"]);
  });

  it("hides tunnels without a saved secret", () => {
    const empty = tunnelView({
      id: "empty",
      credentials: {
        authType: "password",
        hasPassword: false,
        hasPrivateKey: false,
        hasPassphrase: false,
      },
    });
    expect(selectableJumpTunnels([empty], null, [])).toEqual([]);
  });

  it("hides a tunnel whose own jump chain would exceed the limit", () => {
    const long = tunnelView({
      id: "long",
      jumps: Array.from({ length: MAX_JUMPS }, (_, index) => ({
        id: `j${index}`,
        label: "",
        host: `hop${index}`,
        port: 22,
        username: "ops",
        credentials: {
          authType: "password" as const,
          hasPassword: true,
          hasPrivateKey: false,
          hasPassphrase: false,
        },
        inheritedFrom: null,
      })),
    });
    expect(selectableJumpTunnels([long], null, [])).toEqual([]);
  });
});
