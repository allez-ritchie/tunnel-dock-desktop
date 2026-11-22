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

  it("explains why no endpoint could be dialed", () => {
    expect(describeError("no_endpoint", null, null, "家庭 NAS")).toContain(
      "家庭 NAS",
    );
    expect(describeError("all_endpoints_disabled", null)).toBe(
      strings.errorAllEndpointsDisabled,
    );
    expect(describeError("wifi_only_endpoint", null)).toBe(
      strings.errorWifiOnlyEndpoint,
    );
    expect(describeError("endpoint_incomplete", null)).toBe(
      strings.errorEndpointIncomplete,
    );
    // 不知道隧道名时也不能出现 undefined。
    expect(describeError("no_endpoint", null)).not.toContain("undefined");
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

  it("summarizes bundle counts and skips without placeholders", () => {
    expect(strings.settingsBundleCounts(3, 5, 2)).toBe(
      "将导出 2 个应用组、5 个应用、3 条隧道",
    );
    expect(strings.bundleImported(1, 2, 3)).toContain("1");
    expect(strings.bundleImportedSkipped(2, 1)).toContain("2");
    expect(strings.bundleImportedConflicts("后台（18080）")).toContain("18080");
    expect(strings.settingsBundleExportSkipped("NAS、DB")).toContain("NAS");
    expect(strings.bundlePreviewBody(1, 2, 3)).toContain("3");
    expect(strings.bundlePreviewMissingKeys(2)).toContain("2");
    expect(strings.bundleImportedNoKey(3)).toContain("3");
  });

  it("summarizes multi-line tunnels and fallbacks", () => {
    expect(strings.endpointCount(2)).toBe("2 条线路");
    expect(strings.endpointCountSuffix(3)).toContain("3");
    expect(strings.endpointCount(4)).toContain("4");
    expect(strings.tunnelEndpointCount(2)).toBe("2 条线路");
    expect(strings.endpointInUse("frp 外网")).toContain("frp 外网");
    expect(strings.endpointFallbackUsed("frp 外网")).toContain("frp 外网");
    expect(strings.endpointDialableHint(1, 2)).toContain("1");
    expect(strings.endpointFailureLine("家庭局域网", "连接超时")).toContain(
      "连接超时",
    );
    expect(strings.endpointPreferredOn("家庭局域网")).toContain("家庭局域网");
    expect(strings.endpointDefaultLabel(2)).toBe("线路 2");
    expect(strings.tunnelTestOkVia("外网", 120)).toContain("外网");
  });

  it("describes ~/.ssh config import states", () => {
    expect(strings.sshConfigFound(3, 3)).toBe("共 3 个主机，全部可直接导入");
    expect(strings.sshConfigFound(4, 2)).toBe("共 4 个主机，其中 2 个可直接导入");
    expect(strings.sshConfigImported(2)).toContain("2");
    expect(strings.sshConfigSelected(0)).toContain("0");
    expect(strings.localKeySelected("id_ed25519")).toContain("id_ed25519");
    expect(strings.sshConfigUnsupported("ProxyCommand")).toContain("ProxyCommand");
  });
});
