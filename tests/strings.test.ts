import { describe, expect, it } from "vitest";
import { describeError, strings } from "../src/strings";

describe("describeError", () => {
  it("maps the security error to the fingerprint guidance", () => {
    expect(describeError("host_key_changed", null)).toBe(
      strings.errorHostKeyChanged,
    );
  });

  it("substitutes the port into the port-in-use message", () => {
    expect(describeError("port_in_use", null, 18080)).toBe(
      strings.errorPortInUse(18080),
    );
    expect(describeError("port_in_use", null)).toBe(strings.errorPortInUse(0));
  });

  it("maps dangling references to actionable copy", () => {
    expect(describeError("tunnel_deleted", null)).toBe(
      strings.errorTunnelDeleted,
    );
    expect(describeError("group_missing", null)).toBe(strings.errorGroupMissing);
    expect(describeError("port_conflict", null)).toBe(
      strings.errorPortConflict,
    );
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
      "tunnel_down",
      "forward_failed",
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
    expect(describeError("weird_code", null)).toBe(
      strings.errorGeneric("未知错误"),
    );
  });
});

describe("strings", () => {
  it("formats counts without leaking placeholders", () => {
    expect(strings.appsGroupForwarding(3)).toBe("3 个转发中");
    expect(strings.appsGroupCount(2)).toBe("2 个应用");
    expect(strings.tunnelAppsUsing(2)).toBe("被 2 个应用使用");
    expect(strings.tunnelJumpCount(2)).toBe("2 级跳板");
    expect(strings.appRetryCount(5)).toBe("第 5 次");
    expect(strings.appNextRetry(8)).toBe("8 秒后重试");
    expect(strings.settingsVersion("1.0.0")).toBe("版本 1.0.0");
  });

  it("quotes user-supplied names in destructive copy", () => {
    expect(strings.appDeleteBody("公司后台")).toContain("公司后台");
    expect(strings.groupDeleteBody("工作", 2)).toContain("工作");
    expect(strings.groupDeleteBody("工作", 2)).toContain("2");
    expect(strings.tunnelDeleteBody("家庭 NAS", 1)).toContain("家庭 NAS");
    expect(strings.toastTunnelDeleted(2)).toContain("2");
  });
});
