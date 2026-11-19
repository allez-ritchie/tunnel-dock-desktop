import { describe, expect, it } from "vitest";
import type { JumpInput, TunnelInput, TunnelView } from "../src/api";
import {
  MAX_JUMPS,
  chainPreview,
  formFrom,
  hasSavedPassword,
  jumpError,
  jumpFormFrom,
  nextPort,
  toInput,
  toJumpInput,
  validate,
  type JumpForm,
  type TunnelForm,
} from "../src/draft";
import { strings } from "../src/strings";

function form(overrides: Partial<TunnelForm> = {}): TunnelForm {
  return {
    ...formFrom(undefined, 18080),
    name: "家庭 NAS",
    remoteHost: "127.0.0.1",
    remotePort: "5000",
    sshHost: "example.com",
    sshPort: "22",
    sshUser: "ops",
    authType: "password",
    passwordInput: "s3cret",
    ...overrides,
  };
}

function view(overrides: Partial<TunnelView> = {}): TunnelView {
  return {
    id: "t1",
    name: "NAS",
    localHost: "127.0.0.1",
    localPort: 18080,
    remoteHost: "127.0.0.1",
    remotePort: 5000,
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
    autoStartOnBoot: false,
    keepAliveSeconds: 30,
    connectTimeoutSeconds: 15,
    strictHostKey: true,
    browserUrl: "",
    desiredOn: false,
    createdAt: 1,
    updatedAt: 1,
    localUrl: "http://127.0.0.1:18080/",
    browserOpenUrl: "http://127.0.0.1:18080/",
    chainSummary: "example.com",
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

  it("skips ports already taken by other tunnels", () => {
    const tunnels = [
      view({ id: "a", localPort: 18080 }),
      view({ id: "b", localPort: 18081 }),
      view({ id: "c", localPort: 18083 }),
    ];
    expect(nextPort(tunnels)).toBe(18082);
  });
});

describe("validate", () => {
  it("accepts a complete form", () => {
    expect(validate(form(), [])).toEqual({});
  });

  it("reports the fields a brand-new form is missing", () => {
    const errors = validate(formFrom(), []);
    expect(errors.name).toBe(strings.errorNameRequired);
    expect(errors.sshHost).toBe(strings.errorSshHostRequired);
    expect(errors.password).toBe(strings.errorPasswordRequired);
    // Defaults that the editor pre-fills must not raise errors on their own.
    expect(errors.remoteHost).toBeUndefined();
    expect(errors.sshUser).toBeUndefined();
    expect(errors.localPort).toBeUndefined();
  });

  it("rejects names longer than 40 characters", () => {
    const errors = validate(form({ name: "名".repeat(41) }), []);
    expect(errors.name).toBe(strings.errorNameTooLong);
  });

  it("counts unicode characters, not bytes, for the name limit", () => {
    expect(validate(form({ name: "名".repeat(40) }), []).name).toBeUndefined();
  });

  it("rejects privileged local ports", () => {
    expect(validate(form({ localPort: "80" }), []).localPort).toBe(
      strings.errorLocalPort,
    );
  });

  it("rejects non-numeric and out-of-range ports", () => {
    expect(validate(form({ localPort: "abc" }), []).localPort).toBe(
      strings.errorLocalPort,
    );
    expect(validate(form({ remotePort: "70000" }), []).remotePort).toBe(
      strings.errorRemotePort,
    );
    expect(validate(form({ sshPort: "0" }), []).sshPort).toBe(
      strings.errorSshPort,
    );
  });

  it("reports a local-port conflict against another tunnel", () => {
    const errors = validate(form(), [view({ id: "other", name: "别的", localPort: 18080 })]);
    expect(errors.localPort).toBe(strings.errorLocalPortInUse("别的"));
  });

  it("does not self-conflict when editing the same tunnel", () => {
    const errors = validate(form({ id: "self" }), [view({ id: "self", localPort: 18080 })]);
    expect(errors.localPort).toBeUndefined();
  });

  it("rejects hostnames containing whitespace", () => {
    expect(validate(form({ sshHost: "bad host" }), []).sshHost).toBe(
      strings.errorSshHostRequired,
    );
    expect(validate(form({ remoteHost: "10.0.0.1 x" }), []).remoteHost).toBe(
      strings.errorRemoteHostRequired,
    );
  });

  it("enforces keepalive and timeout bounds", () => {
    expect(validate(form({ keepAliveSeconds: "4" }), []).keepAliveSeconds).toBe(
      strings.errorKeepalive,
    );
    expect(validate(form({ keepAliveSeconds: "301" }), []).keepAliveSeconds).toBe(
      strings.errorKeepalive,
    );
    expect(validate(form({ connectTimeoutSeconds: "121" }), []).connectTimeoutSeconds).toBe(
      strings.errorTimeout,
    );
    expect(validate(form({ keepAliveSeconds: "5", connectTimeoutSeconds: "120" }), [])).toEqual(
      {},
    );
  });

  it("checks the browser URL scheme case-insensitively", () => {
    expect(validate(form({ browserUrl: "HTTPS://127.0.0.1:{port}/" }), [])).toEqual({});
    expect(validate(form({ browserUrl: "ftp://127.0.0.1/" }), []).browserUrl).toBe(
      strings.errorBrowserUrl,
    );
  });

  it("requires a secret only when none is saved", () => {
    const withoutSaved = form({ passwordInput: "", hasPassword: false });
    expect(validate(withoutSaved, []).password).toBe(strings.errorPasswordRequired);

    const withSaved = form({ passwordInput: "", hasPassword: true });
    expect(validate(withSaved, []).password).toBeUndefined();

    const cleared = form({ passwordInput: "", hasPassword: true, passwordCleared: true });
    expect(validate(cleared, []).password).toBe(strings.errorPasswordRequired);
  });

  it("requires a private key when key auth is selected", () => {
    const errors = validate(form({ authType: "private_key", privateKeyInput: "" }), []);
    expect(errors.privateKey).toBe(strings.errorPrivateKeyRequired);
  });

  it("rejects more than the jump limit", () => {
    const jumps = Array.from({ length: MAX_JUMPS + 1 }, () => jump());
    expect(validate(form({ jumps }), []).jumps).toBe(strings.errorMaxJumps);
  });

  it("rejects an incomplete jump inside the list", () => {
    const errors = validate(form({ jumps: [jump({ host: "" })] }), []);
    expect(errors.jumps).toBe(strings.errorJumpInvalid);
  });
});

describe("jumpError", () => {
  it("accepts a complete password jump", () => {
    expect(jumpError(jump())).toBeNull();
  });

  it("validates host, port, username and secret", () => {
    expect(jumpError(jump({ host: " " }))).toBe(strings.errorJumpHostRequired);
    expect(jumpError(jump({ port: "0" }))).toBe(strings.errorJumpPort);
    expect(jumpError(jump({ username: "" }))).toBe(strings.errorJumpUsernameRequired);
    expect(jumpError(jump({ passwordInput: "" }))).toBe(strings.errorJumpPasswordRequired);
  });

  it("accepts a saved secret that was not re-entered", () => {
    expect(jumpError(jump({ passwordInput: "", hasPassword: true }))).toBeNull();
  });

  it("requires a private key for key auth", () => {
    expect(jumpError(jump({ authType: "private_key", privateKeyInput: "" }))).toBe(
      strings.errorJumpPrivateKeyRequired,
    );
  });
});

describe("toInput", () => {
  it("sends null for untouched secrets so the backend keeps them", () => {
    const input = toInput(form({ id: "t1", hasPassword: true, passwordInput: "" }));
    expect(input.credentials.password).toBeNull();
    expect(input.credentials.privateKey).toBeNull();
    expect(input.credentials.passphrase).toBeNull();
  });

  it("sends an empty string when the user cleared a stored secret", () => {
    const input = toInput(form({ id: "t1", hasPassword: true, passwordCleared: true }));
    expect(input.credentials.password).toBe("");
  });

  it("sends the new secret when the user typed one", () => {
    const input = toInput(form({ passwordInput: "new-secret" }));
    expect(input.credentials.password).toBe("new-secret");
  });

  it("parallelizes jump credentials", () => {
    const input = toInput(
      form({
        jumps: [
          jump({ passwordInput: "" , hasPassword: true }),
          jump({ passwordInput: "typed" }),
        ],
      }),
    );
    expect(input.jumps[0].credentials.password).toBeNull();
    expect(input.jumps[1].credentials.password).toBe("typed");
  });

  it("trims text fields and pins the listener to loopback", () => {
    const input = toInput(
      form({
        name: "  NAS  ",
        sshHost: " example.com ",
        sshUser: " ops ",
        remoteHost: " 10.0.0.5 ",
        browserUrl: " http://127.0.0.1:{port}/ ",
      }),
    );
    expect(input.name).toBe("NAS");
    expect(input.sshHost).toBe("example.com");
    expect(input.sshUser).toBe("ops");
    expect(input.remoteHost).toBe("10.0.0.5");
    expect(input.browserUrl).toBe("http://127.0.0.1:{port}/");
    expect(input.localHost).toBe("127.0.0.1");
  });

  it("converts numeric strings to numbers", () => {
    const input: TunnelInput = toInput(
      form({ localPort: "18080", remotePort: "5000", sshPort: "2222", keepAliveSeconds: "45" }),
    );
    expect(input.localPort).toBe(18080);
    expect(input.remotePort).toBe(5000);
    expect(input.sshPort).toBe(2222);
    expect(input.keepAliveSeconds).toBe(45);
  });

  it("keeps the jump order stable when serializing", () => {
    const input = toInput(
      form({ jumps: [jump({ label: "first" }), jump({ label: "second" })] }),
    );
    expect(input.jumps.map((item: JumpInput) => item.label)).toEqual(["first", "second"]);
  });
});

describe("toJumpInput", () => {
  it("preserves the jump id when editing", () => {
    const input = toJumpInput(jump({ id: "jump-1" }));
    expect(input.id).toBe("jump-1");
  });

  it("leaves the id null for a new jump", () => {
    expect(toJumpInput(jump()).id).toBeNull();
  });
});

describe("formFrom", () => {
  it("suggests the next free port for a new tunnel", () => {
    expect(formFrom(undefined, 18083).localPort).toBe("18083");
  });

  it("never exposes stored secrets into the form", () => {
    const restored = formFrom(view());
    expect(restored.hasPassword).toBe(true);
    expect(restored.passwordInput).toBe("");
    expect(restored.passwordCleared).toBe(false);
    expect(hasSavedPassword(restored)).toBe(true);
  });

  it("round-trips an existing tunnel into the form", () => {
    const source = view({
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
        },
      ],
    });
    const restored = formFrom(source);
    expect(restored.jumps).toHaveLength(1);
    expect(restored.jumps[0].host).toBe("bastion");
    expect(restored.jumps[0].authType).toBe("private_key");
    expect(restored.jumps[0].hasPrivateKey).toBe(true);
    expect(restored.jumps[0].hasPassphrase).toBe(true);
    expect(restored.jumps[0].privateKeyInput).toBe("");
  });
});

describe("chainPreview", () => {
  it("falls back to the SSH server label when empty", () => {
    expect(chainPreview(formFrom(undefined))).toBe("SSH 服务器");
  });

  it("lists jumps in order before the target", () => {
    const preview = chainPreview(
      form({
        sshHost: "server",
        jumps: [jump({ label: "A", host: "a" }), jump({ label: "", host: "b" })],
      }),
    );
    expect(preview).toBe("A  →  b  →  server");
  });
});
