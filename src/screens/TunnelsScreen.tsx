import { useMemo, useState } from "react";
import type { AppStatus, AppView, TunnelTestResult, TunnelView } from "../api";
import {
  BrandMark,
  Chip,
  ConfirmDialog,
  Icon,
  SectionHeader,
} from "../components/ui";
import { describeError, strings } from "../strings";

export type TestEntry = TunnelTestResult | "testing";

/** 首选线路的跳板级数；卡片上的“N 级跳板”只说这一条，避免误导。 */
function primaryJumps(tunnel: TunnelView): number {
  const preferred =
    tunnel.endpoints.find((item) => item.id === tunnel.preferredEndpointId) ??
    tunnel.endpoints[0];
  return preferred?.jumps.length ?? 0;
}

interface Props {
  tunnels: TunnelView[];
  apps: AppView[];
  statuses: Record<string, AppStatus>;
  tests: Record<string, TestEntry>;
  onTest: (id: string) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onCreate: () => void;
  /** 打开「导入 ~/.ssh/config」页面。 */
  onImportSshConfig: () => void;
}

export function TunnelsScreen({
  tunnels,
  apps,
  statuses,
  tests,
  onTest,
  onEdit,
  onDelete,
  onCreate,
  onImportSshConfig,
}: Props) {
  const [deleteConfirm, setDeleteConfirm] = useState<TunnelView | null>(null);

  const rows = useMemo(
    () =>
      tunnels.map((tunnel) => {
        const usedBy = apps.filter((app) => app.tunnelId === tunnel.id);
        return {
          tunnel,
          usedBy,
          running: usedBy.filter(
            (app) => statuses[app.id]?.phase === "connected",
          ).length,
        };
      }),
    [apps, statuses, tunnels],
  );

  const totalAppsUsing = rows.reduce((sum, row) => sum + row.usedBy.length, 0);
  const totalRunning = rows.reduce((sum, row) => sum + row.running, 0);

  return (
    <div className="screen tab-screen tunnels-screen">
      <header className="page-header">
        <div className="brand">
          <BrandMark size={44} />
          <div>
            <span className="brand-kicker">{strings.tunnelsKicker}</span>
            <h1>{strings.tunnelsTitle}</h1>
            <p className="brand-tagline">{strings.tunnelsSubtitle}</p>
          </div>
        </div>
        <div className="page-header-actions">
          <button
            type="button"
            className="btn outline sm"
            onClick={onImportSshConfig}
          >
            <Icon name="terminal" size={16} />
            {strings.sshConfigEntry}
          </button>
          <button type="button" className="btn primary sm" onClick={onCreate}>
            <Icon name="plus" size={16} />
            {strings.tunnelsAdd}
          </button>
        </div>
      </header>

      <section className="stat-strip">
        <div>
          <strong>{tunnels.length}</strong>
          <span>{strings.tunnelsTitle}</span>
        </div>
        <div>
          <strong>{totalAppsUsing}</strong>
          <span>{strings.appsTitle}</span>
        </div>
        <div>
          <strong style={{ color: "var(--online)" }}>{totalRunning}</strong>
          <span>{strings.appsStatForwarding}</span>
        </div>
        <div className="stat-strip-hint">
          <Icon name="route" size={16} />
          <span>{strings.tunnelsSubtitle}</span>
        </div>
      </section>

      {tunnels.length === 0 ? (
        <section className="empty-state">
          <BrandMark size={62} />
          <h2>{strings.tunnelsEmptyTitle}</h2>
          <p>{strings.tunnelsEmptyBody}</p>
          <button type="button" className="btn primary" onClick={onCreate}>
            <Icon name="plus" size={17} />
            {strings.tunnelsAdd}
          </button>
        </section>
      ) : (
        <>
          <div className="content-head">
            <SectionHeader
              title={strings.tunnelsTitle}
              hint={strings.tunnelsSubtitle}
              trailing={
                <span className="count-pill">
                  {tunnels.length}
                </span>
              }
            />
          </div>
          <div className="tunnel-stack">
            {rows.map(({ tunnel, usedBy, running }) => {
              const test = tests[tunnel.id];
              return (
                <article key={tunnel.id} className="tunnel-card tunnel-row">
                  <div className="tunnel-row-main">
                    <div className="tunnel-row-head">
                      <h3 className="card-title">{tunnel.name}</h3>
                      <div className="card-chips">
                        <Chip color="var(--secondary)" icon="link">
                          {strings.tunnelAppsUsing(usedBy.length)}
                        </Chip>
                        {running > 0 ? (
                          <Chip color="var(--online)" icon="pulse">
                            {strings.tunnelRunning(running)}
                          </Chip>
                        ) : null}
                      </div>
                    </div>
                    <p className="card-ssh mono">{tunnel.endpoint}</p>
                    <p className="tunnel-chain mono">
                      <Icon name="route" size={13} />
                      {tunnel.chainSummary}
                    </p>
                    <div className="card-chips">
                      {tunnel.endpoints.length > 1 ? (
                        <Chip color="var(--primary)" icon="route">
                          {strings.tunnelEndpointCount(tunnel.endpoints.length)}
                        </Chip>
                      ) : null}
                      <Chip icon="terminal">
                        {strings.tunnelKeepalive(tunnel.keepAliveSeconds)}
                      </Chip>
                      {tunnel.dialableCount === 0 ? (
                        <Chip color="var(--danger)" icon="alert">
                          {strings.errorEndpointIncomplete}
                        </Chip>
                      ) : tunnel.dialableCount < tunnel.endpoints.length ? (
                        <Chip color="var(--connecting)" icon="info">
                          {strings.endpointDialableHint(
                            tunnel.dialableCount,
                            tunnel.endpoints.length,
                          )}
                        </Chip>
                      ) : null}
                      {primaryJumps(tunnel) === 0 ? (
                        <Chip icon="shield">{strings.editorDirect}</Chip>
                      ) : (
                        <Chip color="var(--secondary)" icon="route">
                          {strings.tunnelJumpCount(primaryJumps(tunnel))}
                        </Chip>
                      )}
                      {tunnel.autoReconnect ? (
                        <Chip color="var(--online)" icon="refresh">
                          {strings.editorAutoReconnect}
                        </Chip>
                      ) : null}
                    </div>

                    {/* 多条线路时逐条列出：用户一眼就能看出首选是哪个入口。 */}
                    {tunnel.endpoints.length > 1 ? (
                      <ul className="tunnel-endpoint-list">
                        {tunnel.endpoints.map((endpoint) => (
                          <li
                            key={endpoint.id}
                            className={`tunnel-endpoint${
                              endpoint.id === tunnel.preferredEndpointId
                                ? " is-preferred"
                                : ""
                            }${endpoint.enabled ? "" : " is-disabled"}`}
                          >
                            <span className="tunnel-endpoint-name">
                              {endpoint.id === tunnel.preferredEndpointId ? (
                                <Icon name="star" size={12} />
                              ) : null}
                              {endpoint.displayName}
                            </span>
                            <span className="mono">{endpoint.endpoint}</span>
                            {endpoint.wifiOnly ? (
                              <em>{strings.endpointWifiOnlyBadge}</em>
                            ) : null}
                            {endpoint.enabled ? null : (
                              <em className="is-danger">
                                {strings.endpointDisabledBadge}
                              </em>
                            )}
                          </li>
                        ))}
                      </ul>
                    ) : null}

                    {test === "testing" ? (
                      <div className="test-result is-testing">
                        <span className="spinner" />
                        {strings.tunnelTesting}
                      </div>
                    ) : test ? (
                      <div
                        className={`test-result ${
                          test.state === "success" ? "is-ok" : "is-bad"
                        }`}
                      >
                        <Icon
                          name={test.state === "success" ? "check" : "alert"}
                          size={16}
                        />
                        <span>
                          {test.state === "success"
                            ? test.endpointLabel
                              ? strings.tunnelTestOkVia(
                                  test.endpointLabel,
                                  test.elapsedMs,
                                )
                              : strings.tunnelTestOk(test.elapsedMs)
                            : `${strings.tunnelTestFail}：${describeError(
                                test.errorCode,
                                test.errorDetail,
                                null,
                                tunnel.name,
                              )}`}
                        </span>
                        {test.securityAlert ? (
                          <em>{strings.tunnelTestSecurity}</em>
                        ) : null}
                        {/* 逐线路的失败明细：多入口时“哪条路不通”才是关键信息。 */}
                        {test.state === "failure" &&
                        test.endpointFailures.length > 0 ? (
                          <ul className="test-failures">
                            {test.endpointFailures.map((failure) => (
                              <li key={failure.endpointId}>
                                {strings.endpointFailureLine(
                                  failure.label,
                                  describeError(
                                    failure.code,
                                    failure.detail,
                                    null,
                                    tunnel.name,
                                  ),
                                )}
                              </li>
                            ))}
                          </ul>
                        ) : null}
                      </div>
                    ) : null}

                    {usedBy.length > 0 ? (
                      <p className="tunnel-used mono">
                        {usedBy.map((app) => app.name).join(" · ")}
                      </p>
                    ) : (
                      <p className="tunnel-used muted">{strings.tunnelNoApps}</p>
                    )}
                  </div>

                  <div className="tunnel-row-actions">
                    <button
                      type="button"
                      className="btn outline sm"
                      onClick={() => onTest(tunnel.id)}
                      disabled={test === "testing"}
                    >
                      {test === "testing" ? (
                        <span className="spinner" />
                      ) : (
                        <Icon name="speed" size={15} />
                      )}
                      {test === "testing"
                        ? strings.tunnelTesting
                        : strings.tunnelTest}
                    </button>
                    <button
                      type="button"
                      className="btn outline sm"
                      onClick={() => onEdit(tunnel.id)}
                    >
                      <Icon name="edit" size={15} />
                      {strings.tunnelEdit}
                    </button>
                    <button
                      type="button"
                      className="btn outline sm danger-outline"
                      onClick={() => setDeleteConfirm(tunnel)}
                    >
                      <Icon name="trash" size={15} />
                      {strings.tunnelDelete}
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        </>
      )}

      <p className="oss-line">{strings.settingsOss}</p>

      {deleteConfirm ? (
        <ConfirmDialog
          title={strings.tunnelDeleteTitle}
          body={
            (rows.find((row) => row.tunnel.id === deleteConfirm.id)?.usedBy.length ??
              0) > 0
              ? strings.tunnelDeleteBody(
                  deleteConfirm.name,
                  rows.find((row) => row.tunnel.id === deleteConfirm.id)?.usedBy
                    .length ?? 0,
                )
              : strings.tunnelDeleteBodyUnused(deleteConfirm.name)
          }
          confirmLabel={strings.tunnelDeleteConfirm}
          onCancel={() => setDeleteConfirm(null)}
          onConfirm={() => {
            const id = deleteConfirm.id;
            setDeleteConfirm(null);
            onDelete(id);
          }}
        />
      ) : null}
    </div>
  );
}
