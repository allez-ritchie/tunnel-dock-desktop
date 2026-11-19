import { describe, expect, it } from "vitest";
import { strings, describeError } from "../src/strings";

describe("describeError", () => {
  it("maps the security error to the fingerprint guidance", () => {
    expect(describeError("host_key_changed", null)).toBe(strings.errorHostKeyChanged);
  });

  it("treats a changed host key as a security alert even with no detail", () => {
    expect(describeError("host_key_changed", "")).toContain("主机密钥");
  });

  it("substitutes the port into the port-in-use message", () => {
    expect(describeError("port_in_use", null, 18080)).toBe(
      strings.errorPortInUse(18080),
    );
    expect(describeError("port_in_use", null, 18080)).toContain("18080");
  });

  it("falls back to port 0 when the status carries no port", () => {
    expect(describeError("port_in_use", null)).toBe(strings.errorPortInUse(0));
    expect(describeError("port_in_use", null, null)).toBe(strings.errorPortInUse(0));
  });

  it("maps every stable backend error code", () => {
    const codes = [
      "secret_decrypt",
      "auth_failed",
      "unknown_host",
      "timeout",
      "connection_refused",
      "local_forward_lost",
      "connection_closed",
    ];
    for (const code of codes) {
      const text = describeError(code, null);
      expect(text).not.toBe(strings.errorGeneric("未知错误"));
      expect(text.length).toBeGreaterThan(0);
    }
  });

  it("wraps unknown codes with the raw detail", () => {
    expect(describeError("generic", "connection reset by peer")).toBe(
      strings.errorGeneric("connection reset by peer"),
    );
  });

  it("shows a placeholder when an unknown code carries no detail", () => {
    expect(describeError("weird_code", null)).toBe(strings.errorGeneric("未知错误"));
  });
});

describe("strings", () => {
  it("formats counts and ports without leaking placeholders", () => {
    expect(strings.homeTunnelsCount(3)).toBe("3 条");
    expect(strings.cardJumps(2)).toBe("2 级跳板");
    expect(strings.cardKeepalive(30)).toBe("保活 30 秒");
    expect(strings.cardRetryCount(5)).toBe("第 5 次");
    expect(strings.cardNextRetry(8)).toBe("8 秒后重试");
    expect(strings.settingsVersion("1.0.0")).toBe("版本 1.0.0");
  });

  it("quotes user-supplied names in destructive copy", () => {
    expect(strings.deleteBody("家庭 NAS")).toContain("家庭 NAS");
    expect(strings.errorLocalPortInUse("别的")).toContain("别的");
  });
});
