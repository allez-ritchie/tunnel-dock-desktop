import { describe, expect, it } from "vitest";
import type {
  AppView,
  GroupView,
  JumpInput,
  TunnelView,
} from "../src/api";
import {
  MAX_JUMPS,
  appFormFrom,
  chainPreview,
  hasSavedPassword,
  jumpError,
  jumpFormFrom,
  nextPort,
  toAppInput,
  toTunnelInput,
  tunnelFormFrom,
  usesLocalKey,
  validateApp,
  validateTunnel,
  type AppForm,
  type JumpForm,
  type TunnelForm,
} from "../src/draft";
import { strings } from "../src/strings";

function tunnelForm(overrides: Partial<TunnelForm> = {}): TunnelForm {
  return {
    ...tunnelFormFrom(undefined),
    name: "家庭 NAS",
    sshHost: "example.com",
    sshPort: "22",
    sshUser: "ops",
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
    tunnelId: "t1",
    localPort: "18080",
    remoteHost: "127.0.0.1",
    remotePort: "5000",
    ...overrides,
  };
}

function tunnelView(overrides: Partial<TunnelView> = {}): TunnelView {
  return {
    id: "t1",
    name: "NAS",
    sshHost: "example.com",
    sshPort: 22,
    sshUser: "ops",
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

describe("validateTunnel", () => {
  it("accepts a complete form", () => {
    expect(validateTunnel(tunnelForm())).toEqual({});
  });

  it("reports the fields a brand-new form is missing", () => {
    const errors = validateTunnel(tunnelFormFrom());
    expect(errors.name).toBe(strings.errorNameRequired);
    expect(errors.sshHost).toBe(strings.errorSshHostRequired);
    expect(errors.password).toBe(strings.errorPasswordRequired);
    // Defaults that the editor pre-fills must not raise errors on their own.
    expect(errors.sshUser).toBeUndefined();
    expect(errors.sshPort).toBeUndefined();
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

  it("rejects non-numeric and out-of-range ports", () => {
    expect(validateTunnel(tunnelForm({ sshPort: "abc" })).sshPort).toBe(
      strings.errorSshPort,
    );
    expect(validateTunnel(tunnelForm({ sshPort: "0" })).sshPort).toBe(
      strings.errorSshPort,
    );
  });

  it("rejects hostnames containing whitespace", () => {
    expect(validateTunnel(tunnelForm({ sshHost: "bad host" })).sshHost).toBe(
      strings.errorSshHostRequired,
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

  it("requires a secret only when none is saved", () => {
    const withoutSaved = tunnelForm({ passwordInput: "", hasPassword: false });
    expect(validateTunnel(withoutSaved).password).toBe(
      strings.errorPasswordRequired,
    );

    const withSaved = tunnelForm({ passwordInput: "", hasPassword: true });
    expect(validateTunnel(withSaved).password).toBeUndefined();

    const cleared = tunnelForm({
      passwordInput: "",
      hasPassword: true,
      passwordCleared: true,
    });
    expect(validateTunnel(cleared).password).toBe(strings.errorPasswordRequired);
  });

  it("requires a private key when key auth is selected", () => {
    const errors = validateTunnel(
      tunnelForm({ authType: "private_key", privateKeyInput: "" }),
    );
    expect(errors.privateKey).toBe(strings.errorPrivateKeyRequired);
  });

  it("accepts a locally selected ~/.ssh key without pasting its content", () => {
    const form = tunnelForm({
      authType: "private_key",
      privateKeyInput: "",
      privateKeyPath: "id_ed25519",
    });
    expect(validateTunnel(form).privateKey).toBeUndefined();
  });

  it("rejects more than the jump limit", () => {
    const jumps = Array.from({ length: MAX_JUMPS + 1 }, () => jump());
    expect(validateTunnel(tunnelForm({ jumps })).jumps).toBe(
      strings.errorMaxJumps,
    );
  });

  it("rejects an incomplete jump inside the list", () => {
    expect(
      validateTunnel(tunnelForm({ jumps: [jump({ host: "" })] })).jumps,
    ).toBe(strings.errorJumpInvalid);
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
    expect(toTunnelInput(tunnelForm({ passwordInput: "new-secret" })).credentials.password).toBe(
      "new-secret",
    );
  });

  it("parallelizes jump credentials", () => {
    const input = toTunnelInput(
      tunnelForm({
        jumps: [
          jump({ passwordInput: "", hasPassword: true }),
          jump({ passwordInput: "typed" }),
        ],
      }),
    );
    expect(input.jumps[0].credentials.password).toBeNull();
    expect(input.jumps[1].credentials.password).toBe("typed");
  });

  it("trims text fields", () => {
    const input = toTunnelInput(
      tunnelForm({
        name: "  NAS  ",
        sshHost: " example.com ",
        sshUser: " ops ",
      }),
    );
    expect(input.name).toBe("NAS");
    expect(input.sshHost).toBe("example.com");
    expect(input.sshUser).toBe("ops");
  });

  it("converts numeric strings to numbers", () => {
    const input = toTunnelInput(
      tunnelForm({ sshPort: "2222", keepAliveSeconds: "45" }),
    );
    expect(input.sshPort).toBe(2222);
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
        jumps: [jump({ authType: "private_key", passwordInput: "", privateKeyPath: "id_rsa" })],
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
      tunnelForm({ authType: "private_key", privateKeyInput: "-----BEGIN KEY-----" }),
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
    expect(restored.hasPassword).toBe(true);
    expect(restored.passwordInput).toBe("");
    expect(restored.passwordCleared).toBe(false);
    expect(hasSavedPassword(restored)).toBe(true);
  });

  it("starts with no ~/.ssh key selected", () => {
    expect(tunnelFormFrom(undefined).privateKeyPath).toBeNull();
    expect(jumpFormFrom(undefined).privateKeyPath).toBeNull();
    expect(usesLocalKey(tunnelFormFrom(undefined))).toBe(false);
  });

  it("round-trips an existing tunnel into the form", () => {
    const source = tunnelView({
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
    expect(restored.jumps).toHaveLength(1);
    expect(restored.jumps[0].host).toBe("bastion");
    expect(restored.jumps[0].authType).toBe("private_key");
    expect(restored.jumps[0].hasPrivateKey).toBe(true);
    expect(restored.jumps[0].hasPassphrase).toBe(true);
    expect(restored.jumps[0].privateKeyInput).toBe("");
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
  it("falls back to the SSH server label when empty", () => {
    expect(chainPreview(tunnelFormFrom(undefined))).toBe("SSH 服务器");
  });

  it("lists jumps in order before the target", () => {
    const preview = chainPreview(
      tunnelForm({
        sshHost: "server",
        jumps: [
          jump({ label: "A", host: "a" }),
          jump({ label: "", host: "b" }),
        ],
      }),
    );
    expect(preview).toBe("A  →  b  →  server");
  });
});
