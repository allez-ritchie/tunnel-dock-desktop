import { useMemo, useState, type CSSProperties } from "react";
import type { AppStatus, AppView, GroupView, TunnelView } from "../api";
import {
  BrandMark,
  Chip,
  ConfirmDialog,
  Icon,
  MonoAddress,
  SectionHeader,
  StatusDot,
  WarningBox,
  phaseColor,
} from "../components/ui";
import { appUnavailable, resolveAppStatus } from "../status";
import { describeError, strings } from "../strings";

interface Props {
  apps: AppView[];
  tunnels: TunnelView[];
  groups: GroupView[];
  statuses: Record<string, AppStatus>;
  collapsedGroups: Set<string>;
  onToggleGroup: (id: string) => void;
  onStartApp: (id: string) => void;
  onStopApp: (id: string) => void;
  onRestartApp: (id: string) => void;
  onEditApp: (id: string) => void;
  onDeleteApp: (id: string) => void;
  onOpenError: (id: string) => void;
  onOpenBrowser: (url: string) => void;
  onCopy: (text: string) => void;
  onStartGroup: (id: string) => void;
  onStopGroup: (id: string) => void;
  onCreateApp: (groupId: string) => void;
  onCreateTunnel: () => void;
  onCreateGroup: () => void;
  onRenameGroup: (group: GroupView) => void;
  onDeleteGroup: (group: GroupView, appCount: number) => void;
}

export function AppsScreen({
  apps,
  tunnels,
  groups,
  statuses,
  collapsedGroups,
  onToggleGroup,
  onStartApp,
  onStopApp,
  onRestartApp,
  onEditApp,
  onDeleteApp,
  onOpenError,
  onOpenBrowser,
  onCopy,
  onStartGroup,
  onStopGroup,
  onCreateApp,
  onCreateTunnel,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
}: Props) {
  const [deleteTarget, setDeleteTarget] = useState<AppView | null>(null);
  const [groupDelete, setGroupDelete] = useState<GroupView | null>(null);

  const tunnelMap = useMemo(
    () => new Map(tunnels.map((tunnel) => [tunnel.id, tunnel])),
    [tunnels],
  );
  const groupMap = useMemo(
    () => new Map(groups.map((group) => [group.id, group])),
    [groups],
  );

  const items = apps.map((app) => {
    const tunnel = tunnelMap.get(app.tunnelId);
    const group = groupMap.get(app.groupId);
    const status = resolveAppStatus(app, tunnels, groups, statuses);
    return {
      app,
      tunnel,
      group,
      status,
      unavailable: appUnavailable(tunnel, group, status),
    };
  });

  const forwarding = items.filter((item) => item.status.phase === "connected").length;
  const unavailable = items.filter((item) => item.unavailable).length;
  const grouped = groups.map((group) => ({
    group,
    apps: items.filter((item) => item.app.groupId === group.id),
  }));
  const groupDeleteCount = groupDelete
    ? apps.filter((app) => app.groupId === groupDelete.id).length
    : 0;

  return (
    <div className="screen tab-screen apps-screen">
      <header className="page-header">
        <div className="brand">
          <BrandMark size={44} />
          <div>
            <span className="brand-kicker">{strings.appsKicker}</span>
            <h1>{strings.appsTitle}</h1>
            <p className="brand-tagline">{strings.appsSubtitle}</p>
          </div>
        </div>
        <div className="page-header-actions">
          <button type="button" className="btn outline sm" onClick={onCreateGroup}>
            <Icon name="folder" size={16} />
            {strings.appsNewGroup}
          </button>
          <button
            type="button"
            className="btn primary sm"
            onClick={() => onCreateApp(groups[0]?.id ?? "default")}
            disabled={tunnels.length === 0}
            title={
              tunnels.length === 0 ? strings.appsEmptyCreateTunnel : strings.appsAdd
            }
          >
            <Icon name="plus" size={16} />
            {strings.appsAdd}
          </button>
        </div>
      </header>

      <section className="stat-strip">
        <div>
          <strong style={{ color: "var(--online)" }}>{forwarding}</strong>
          <span>{strings.appsStatForwarding}</span>
        </div>
        <div>
          <strong style={{ color: unavailable > 0 ? "var(--danger)" : undefined }}>
            {unavailable}
          </strong>
          <span>{strings.appsStatUnavailable}</span>
        </div>
        <div>
          <strong>{groups.length}</strong>
          <span>{strings.appsStatGroups}</span>
        </div>
        <div className="stat-strip-hint">
          <Icon name="pulse" size={16} />
          <span>
            {forwarding > 0
              ? `${forwarding} / ${apps.length} ${strings.appsStatForwarding}`
              : strings.appsSubtitle}
          </span>
        </div>
      </section>

      {apps.length === 0 ? (
        <section className="empty-state">
          <BrandMark size={62} />
          <h2>{strings.appsEmptyTitle}</h2>
          <p>{strings.appsEmptyBody}</p>
          {tunnels.length === 0 ? (
            <button type="button" className="btn primary" onClick={onCreateTunnel}>
              <Icon name="plus" size={17} />
              {strings.appsEmptyCreateTunnel}
            </button>
          ) : (
            <button
              type="button"
              className="btn primary"
              onClick={() => onCreateApp(groups[0]?.id ?? "default")}
            >
              <Icon name="plus" size={17} />
              {strings.appsAdd}
            </button>
          )}
        </section>
      ) : (
        <>
          <div className="content-head">
            <SectionHeader
              title={strings.appsGroupsSection}
              hint={strings.appsSubtitle}
              trailing={<span className="count-pill">{groups.length}</span>}
            />
          </div>

          <div className="group-stack">
            {grouped.map(({ group, apps: groupItems }) => {
              const collapsed = collapsedGroups.has(group.id);
              const running = groupItems.filter(
                (item) => item.status.phase === "connected",
              ).length;
              const groupUnavailable = groupItems.filter(
                (item) => item.unavailable,
              ).length;
              const startable = groupItems.some(
                (item) => !["connected", "connecting", "reconnecting"].includes(
                  item.status.phase,
                ) && !item.unavailable,
              );
              const stoppable = groupItems.some(
                (item) =>
                  ["connected", "connecting", "reconnecting"].includes(
                    item.status.phase,
                  ) || item.app.desiredOn,
              );
              return (
                <section
                  key={group.id}
                  className={`group-section${collapsed ? " is-collapsed" : ""}`}
                >
                  <header className="group-head">
                    <button
                      type="button"
                      className="group-toggle"
                      onClick={() => onToggleGroup(group.id)}
                      aria-expanded={!collapsed}
                    >
                      <span className="group-caret">
                        <Icon
                          name={collapsed ? "chevronRight" : "chevronDown"}
                          size={18}
                        />
                      </span>
                      <span className="group-name">
                        {group.name}
                        {group.isDefault ? (
                          <em className="group-badge">
                            {strings.appsGroupDefaultBadge}
                          </em>
                        ) : null}
                      </span>
                      <span className="group-meta">
                        {collapsed ? (
                          <span className="group-forwarding">
                            <span className={running > 0 ? "is-live" : ""} />
                            {strings.appsGroupForwarding(running)}
                          </span>
                        ) : (
                          <>
                            <span>{strings.appsGroupCount(groupItems.length)}</span>
                            {running > 0 ? (
                              <span className="group-forwarding">
                                {strings.appsGroupForwarding(running)}
                              </span>
                            ) : null}
                            {groupUnavailable > 0 ? (
                              <span className="group-bad">
                                {strings.appsGroupUnavailable(groupUnavailable)}
                              </span>
                            ) : null}
                          </>
                        )}
                      </span>
                    </button>

                    <div className="group-actions">
                      <button
                        type="button"
                        className="icon-btn sm"
                        title={strings.appsGroupStart}
                        aria-label={strings.appsGroupStart}
                        disabled={!startable}
                        onClick={() => onStartGroup(group.id)}
                      >
                        <Icon name="play" size={15} />
                      </button>
                      <button
                        type="button"
                        className="icon-btn sm"
                        title={strings.appsGroupStop}
                        aria-label={strings.appsGroupStop}
                        disabled={!stoppable}
                        onClick={() => onStopGroup(group.id)}
                      >
                        <Icon name="stop" size={15} />
                      </button>
                      <button
                        type="button"
                        className="icon-btn sm"
                        title={strings.appsGroupAdd}
                        aria-label={strings.appsGroupAdd}
                        onClick={() => onCreateApp(group.id)}
                      >
                        <Icon name="plus" size={16} />
                      </button>
                      {group.isDefault ? null : (
                        <>
                          <button
                            type="button"
                            className="icon-btn sm"
                            title={strings.appsGroupRename}
                            aria-label={strings.appsGroupRename}
                            onClick={() => onRenameGroup(group)}
                          >
                            <Icon name="edit" size={15} />
                          </button>
                          <button
                            type="button"
                            className="icon-btn sm danger-icon"
                            title={strings.appsGroupDelete}
                            aria-label={strings.appsGroupDelete}
                            onClick={() => setGroupDelete(group)}
                          >
                            <Icon name="trash" size={15} />
                          </button>
                        </>
                      )}
                    </div>
                  </header>

                  {collapsed ? null : groupItems.length === 0 ? (
                    <div className="group-empty">
                      <p>{strings.appsGroupEmpty}</p>
                      <button
                        type="button"
                        className="btn outline sm"
                        onClick={() => onCreateApp(group.id)}
                      >
                        <Icon name="plus" size={15} />
                        {strings.appsGroupAdd}
                      </button>
                    </div>
                  ) : (
                    <div className="app-grid">
                      {groupItems.map((item) => (
                        <AppCard
                          key={item.app.id}
                          item={item}
                          onStart={() => onStartApp(item.app.id)}
                          onStop={() => onStopApp(item.app.id)}
                          onRestart={() => onRestartApp(item.app.id)}
                          onEdit={() => onEditApp(item.app.id)}
                          onDelete={() => setDeleteTarget(item.app)}
                          onOpenError={() => onOpenError(item.app.id)}
                          onOpenBrowser={() => onOpenBrowser(item.app.browserOpenUrl)}
                          onCopy={() => onCopy(item.app.localUrl)}
                        />
                      ))}
                    </div>
                  )}
                </section>
              );
            })}
          </div>
        </>
      )}

      <p className="oss-line">{strings.settingsOss}</p>

      {deleteTarget ? (
        <ConfirmDialog
          title={strings.appDeleteTitle}
          body={strings.appDeleteBody(deleteTarget.name)}
          confirmLabel={strings.appDeleteConfirm}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => {
            onDeleteApp(deleteTarget.id);
            setDeleteTarget(null);
          }}
        />
      ) : null}

      {groupDelete ? (
        <ConfirmDialog
          title={strings.groupDeleteTitle}
          body={
            groupDeleteCount > 0
              ? strings.groupDeleteBody(groupDelete.name, groupDeleteCount)
              : strings.groupDeleteEmptyBody(groupDelete.name)
          }
          confirmLabel={strings.groupDeleteConfirm}
          onCancel={() => setGroupDelete(null)}
          onConfirm={() => {
            onDeleteGroup(groupDelete, groupDeleteCount);
            setGroupDelete(null);
          }}
        />
      ) : null}
    </div>
  );
}

interface CardItem {
  app: AppView;
  tunnel: TunnelView | undefined;
  group: GroupView | undefined;
  status: AppStatus;
  unavailable: boolean;
}

function AppCard({
  item,
  onStart,
  onStop,
  onRestart,
  onEdit,
  onDelete,
  onOpenError,
  onOpenBrowser,
  onCopy,
}: {
  item: CardItem;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onOpenError: () => void;
  onOpenBrowser: () => void;
  onCopy: () => void;
}) {
  const { app, tunnel, status, unavailable } = item;
  const color = unavailable ? "var(--danger)" : phaseColor(status.phase);
  const live = ["connected", "connecting", "reconnecting"].includes(status.phase);
  const errorText =
    unavailable || status.errorCode
      ? describeError(
          status.errorCode ?? (unavailable ? "tunnel_deleted" : null),
          status.errorDetail,
          app.localPort,
        )
      : null;

  return (
    <article
      className={`app-card${unavailable ? " is-unavailable" : ""} phase-${status.phase}`}
      style={{ "--phase": color } as CSSProperties}
    >
      <div className="card-top">
        <span className="card-phase">
          <StatusDot phase={status.phase} size={8} />
          <span style={{ color }}>
            {status.phase === "connected"
              ? strings.appPhaseConnected
              : status.phase === "connecting"
                ? strings.appPhaseConnecting
                : status.phase === "reconnecting"
                  ? strings.appPhaseReconnecting
                  : status.phase === "error"
                    ? strings.appPhaseError
                    : strings.appPhaseIdle}
          </span>
          {status.phase === "reconnecting" && status.attempt > 0 ? (
            <em>{strings.appRetryCount(status.attempt)}</em>
          ) : null}
        </span>
        <div className="card-top-tools">
          {unavailable ? (
            <button
              type="button"
              className="app-alert"
              title={strings.appCardViewError}
              aria-label={strings.appCardViewError}
              onClick={onOpenError}
            >
              <Icon name="alert" size={17} />
            </button>
          ) : null}
          <details className="card-menu">
            <summary aria-label={strings.appCardMore} title={strings.appCardMore}>
              <Icon name="more" size={19} />
            </summary>
            <div className="menu-panel">
              <button type="button" onClick={onEdit}>
                <Icon name="edit" size={15} />
                {strings.appCardEdit}
              </button>
              <button
                type="button"
                onClick={onCopy}
                disabled={status.phase !== "connected"}
              >
                <Icon name="copy" size={15} />
                {strings.appCardCopyUrl}
              </button>
              <button type="button" className="danger-item" onClick={onDelete}>
                <Icon name="trash" size={15} />
                {strings.appCardDelete}
              </button>
            </div>
          </details>
        </div>
      </div>

      <h3 className="card-title">{app.name}</h3>
      <p className="card-ssh mono">
        <Icon name="link" size={12} />
        {tunnel
          ? `${tunnel.name} · ${tunnel.endpoint}`
          : strings.errorDetailTunnelMissing}
      </p>

      <div className="route-box">
        <div>
          <span>{strings.appCardLocal}</span>
          <MonoAddress color={color}>127.0.0.1:{app.localPort}</MonoAddress>
        </div>
        <Icon name="arrowRight" size={16} />
        <div>
          <span>{strings.appCardRemote}</span>
          <MonoAddress>
            {app.remoteHost}:{app.remotePort}
          </MonoAddress>
        </div>
        {status.phase === "connected" ? (
          <button
            type="button"
            className="round-action"
            onClick={onOpenBrowser}
            title={strings.appCardOpenBrowser}
            aria-label={strings.appCardOpenBrowser}
          >
            <Icon name="external" size={15} />
          </button>
        ) : null}
      </div>

      <div className="card-chips">
        {tunnel && tunnel.jumps.length > 0 ? (
          <Chip color="var(--secondary)" icon="route">
            {strings.tunnelJumpCount(tunnel.jumps.length)}
          </Chip>
        ) : null}
        {app.autoStartOnBoot ? (
          <Chip color="var(--online)" icon="refresh">
            {strings.appCardAutoBoot}
          </Chip>
        ) : null}
        {item.group ? <Chip icon="folder">{item.group.name}</Chip> : null}
      </div>

      {status.phase === "connecting" ? (
        <div className="connecting-row">
          <span className="spinner" />
          <span>{strings.appPhaseConnecting}</span>
        </div>
      ) : null}

      {status.phase === "reconnecting" ? (
        <WarningBox
          danger={status.securityAlert}
          text={[
            errorText,
            status.nextRetrySeconds !== null
              ? strings.appNextRetry(status.nextRetrySeconds)
              : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        />
      ) : null}

      {unavailable ? (
        <button
          type="button"
          className="error-callout"
          onClick={onOpenError}
        >
          <Icon name="alert" size={16} />
          <span>
            <strong>{strings.appCardUnavailable}</strong>
            <em>{errorText}</em>
          </span>
          <Icon name="chevronRight" size={16} />
        </button>
      ) : null}

      {status.phase === "connected" ? (
        <p className="card-url mono">{app.browserOpenUrl}</p>
      ) : null}

      <div className="card-actions">
        {status.phase === "connected" ? (
          <>
            <button type="button" className="btn primary sm" onClick={onOpenBrowser}>
              <Icon name="external" size={16} />
              {strings.appCardOpenBrowser}
            </button>
            <button type="button" className="btn outline sm" onClick={onStop}>
              {strings.appCardStop}
            </button>
          </>
        ) : live ? (
          <button type="button" className="btn outline sm full" onClick={onStop}>
            {strings.appCardStop}
          </button>
        ) : unavailable ? (
          <>
            <button type="button" className="btn outline sm" onClick={onEdit}>
              <Icon name="edit" size={15} />
              {strings.appCardEdit}
            </button>
            <button
              type="button"
              className="btn primary sm"
              onClick={status.phase === "error" ? onRestart : onStart}
              disabled={!item.tunnel}
              title={
                item.tunnel ? strings.errorDetailRetry : strings.errorTunnelDeleted
              }
            >
              <Icon name="refresh" size={16} />
              {strings.errorDetailRetry}
            </button>
          </>
        ) : (
          <>
            <button type="button" className="btn primary sm" onClick={onStart}>
              <Icon name="play" size={16} />
              {strings.appCardStart}
            </button>
            <button type="button" className="btn outline sm" onClick={onEdit}>
              <Icon name="edit" size={15} />
              {strings.appCardEdit}
            </button>
          </>
        )}
      </div>
    </article>
  );
}
