import { useState, type CSSProperties } from "react";
import type { TunnelStatus, TunnelView } from "../api";
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
  phaseLabel,
} from "../components/ui";
import { describeError, strings } from "../strings";

export interface HomeItem {
  config: TunnelView;
  status: TunnelStatus;
}

interface Props {
  items: HomeItem[];
  onOpenSettings: () => void;
  onCreate: () => void;
  onEdit: (id: string) => void;
  onConnect: (id: string) => void;
  onConnectAll: () => void;
  onDisconnect: (id: string) => void;
  onDisconnectAll: () => void;
  onRetry: (id: string) => void;
  onDelete: (id: string) => void;
  onOpenBrowser: (url: string) => void;
  onMessage: (text: string) => void;
}

export function HomeScreen({
  items,
  onOpenSettings,
  onCreate,
  onEdit,
  onConnect,
  onConnectAll,
  onDisconnect,
  onDisconnectAll,
  onRetry,
  onDelete,
  onOpenBrowser,
  onMessage,
}: Props) {
  const [deleteTarget, setDeleteTarget] = useState<TunnelView | null>(null);
  const [confirmDisconnectAll, setConfirmDisconnectAll] = useState(false);

  const connected = items.filter((item) => item.status.phase === "connected").length;
  const reconnecting = items.filter(
    (item) => item.status.phase === "reconnecting",
  ).length;
  const live = items.filter((item) =>
    ["connected", "connecting", "reconnecting", "stopping"].includes(
      item.status.phase,
    ),
  );
  const totalJumps = items.reduce((sum, item) => sum + item.config.jumps.length, 0);
  const canConnectAll = items.length > 0 && items.some((item) => !isLive(item.status));
  const canDisconnectAll =
    items.length > 0 &&
    (live.length > 0 || items.some((item) => item.config.desiredOn));

  const heroStatus =
    items.length === 0
      ? strings.homeHeroEmpty
      : connected > 0
        ? strings.homeHeroActive(connected)
        : strings.homeHeroIdle;

  async function copyUrl(item: HomeItem) {
    try {
      await navigator.clipboard.writeText(item.config.localUrl);
      onMessage(strings.cardCopied);
    } catch {
      onMessage(strings.cardCopied);
    }
  }

  return (
    <div className="screen home-screen">
      <div className="ambient-glow" />
      <header className="topbar">
        <div className="brand">
          <BrandMark size={44} />
          <div>
            <span className="brand-kicker">TUNNELDOCK</span>
            <span className="brand-tagline">{strings.appTagline}</span>
          </div>
        </div>
        <button
          type="button"
          className="icon-btn settings-btn"
          onClick={onOpenSettings}
          aria-label={strings.homeSettings}
          title={strings.homeSettings}
        >
          <Icon name="settings" size={19} />
        </button>
      </header>

      <section className="hero">
        <div className="hero-rings" aria-hidden="true" />
        <div className="hero-top">
          <Chip
            color="var(--primary)"
            icon="terminal"
          >
            {strings.homeKicker}
          </Chip>
          <Chip color="var(--online)" icon="shield">
            {strings.homeReadyBadge}
          </Chip>
        </div>
        <div className="hero-count">
          <strong>{connected}</strong>
          <span className="hero-total">/ {items.length}</span>
          <span className="hero-online">{strings.homeOnline}</span>
        </div>
        <p className="hero-status">{heroStatus}</p>
        <div className="hero-stats">
          <div>
            <strong style={{ color: "var(--online)" }}>{connected}</strong>
            <span>{strings.homeStatOnline}</span>
          </div>
          <div>
            <strong style={{ color: reconnecting > 0 ? "var(--connecting)" : undefined }}>
              {reconnecting}
            </strong>
            <span>{strings.homeStatReconnect}</span>
          </div>
          <div>
            <strong style={{ color: totalJumps > 0 ? "var(--secondary)" : undefined }}>
              {totalJumps}
            </strong>
            <span>{strings.homeStatJumps}</span>
          </div>
        </div>
        <div className="hero-actions">
          <button
            type="button"
            className="btn primary"
            disabled={!canConnectAll}
            onClick={onConnectAll}
          >
            <Icon name="play" size={17} />
            {strings.homeConnectAll}
          </button>
          <button
            type="button"
            className="btn outline"
            disabled={!canDisconnectAll}
            onClick={() => setConfirmDisconnectAll(true)}
          >
            <Icon name="stop" size={17} />
            {strings.homeDisconnectAll}
          </button>
        </div>
        <p className="hero-hint">{strings.homeBackgroundHint}</p>
      </section>

      <div className="content-head">
        <SectionHeader
          title={strings.homeSectionTunnels}
          hint={strings.homeSectionHint}
          trailing={
            <div className="content-head-actions">
              <span className="count-pill">{strings.homeTunnelsCount(items.length)}</span>
              <button type="button" className="btn primary sm" onClick={onCreate}>
                <Icon name="plus" size={16} />
                {strings.homeAddTunnel}
              </button>
            </div>
          }
        />
      </div>

      {items.length === 0 ? (
        <EmptyState onCreate={onCreate} />
      ) : (
        <div className="tunnel-grid">
          {items.map((item) => (
            <TunnelCard
              key={item.config.id}
              item={item}
              onEdit={() => onEdit(item.config.id)}
              onConnect={() => onConnect(item.config.id)}
              onDisconnect={() => onDisconnect(item.config.id)}
              onRetry={() => onRetry(item.config.id)}
              onOpenBrowser={() => onOpenBrowser(item.config.browserOpenUrl)}
              onCopy={() => void copyUrl(item)}
              onDelete={() => setDeleteTarget(item.config)}
            />
          ))}
        </div>
      )}

      <p className="oss-line">{strings.settingsOss}</p>

      {deleteTarget ? (
        <ConfirmDialog
          title={strings.deleteTitle}
          body={strings.deleteBody(deleteTarget.name)}
          confirmLabel={strings.deleteConfirm}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => {
            onDelete(deleteTarget.id);
            setDeleteTarget(null);
          }}
        />
      ) : null}

      {confirmDisconnectAll ? (
        <ConfirmDialog
          title={strings.bulkDisconnectTitle}
          body={strings.bulkDisconnectBody(Math.max(live.length, 1))}
          confirmLabel={strings.bulkDisconnectConfirm}
          onCancel={() => setConfirmDisconnectAll(false)}
          onConfirm={() => {
            setConfirmDisconnectAll(false);
            onDisconnectAll();
          }}
        />
      ) : null}
    </div>
  );
}

function isLive(status: TunnelStatus): boolean {
  return ["connected", "connecting", "reconnecting", "stopping"].includes(
    status.phase,
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <section className="empty-state">
      <BrandMark size={62} />
      <h2>{strings.homeEmptyTitle}</h2>
      <p>{strings.homeEmptyBody}</p>
      <code>127.0.0.1:18080 → 10.0.0.2:80</code>
      <button type="button" className="btn primary" onClick={onCreate}>
        <Icon name="plus" size={17} />
        {strings.homeAddTunnel}
      </button>
    </section>
  );
}

interface CardProps {
  item: HomeItem;
  onEdit: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onRetry: () => void;
  onOpenBrowser: () => void;
  onCopy: () => void;
  onDelete: () => void;
}

function TunnelCard({
  item,
  onEdit,
  onConnect,
  onDisconnect,
  onRetry,
  onOpenBrowser,
  onCopy,
  onDelete,
}: CardProps) {
  const { config, status } = item;
  const color = phaseColor(status.phase);
  const live = isLive(status);
  const errorText =
    status.errorCode !== null
      ? describeError(status.errorCode, status.errorDetail, config.localPort)
      : null;

  return (
    <article
      className={`tunnel-card phase-${status.phase}`}
      style={{ "--phase": color } as CSSProperties}
    >
      <div className="card-top">
        <span className="card-phase">
          <StatusDot phase={status.phase} size={8} />
          <span style={{ color }}>{phaseLabel(status.phase)}</span>
          {status.phase === "reconnecting" && status.attempt > 0 ? (
            <em>{strings.cardRetryCount(status.attempt)}</em>
          ) : null}
        </span>
        <details className="card-menu">
          <summary aria-label={strings.cardMore} title={strings.cardMore}>
            <Icon name="more" size={19} />
          </summary>
          <div className="menu-panel">
            <button type="button" onClick={onEdit}>
              <Icon name="edit" size={15} />
              {strings.cardEdit}
            </button>
            <button
              type="button"
              onClick={onCopy}
              disabled={status.phase !== "connected"}
            >
              <Icon name="copy" size={15} />
              {strings.cardCopyUrl}
            </button>
            <button type="button" className="danger-item" onClick={onDelete}>
              <Icon name="trash" size={15} />
              {strings.cardDelete}
            </button>
          </div>
        </details>
      </div>

      <h3 className="card-title">{config.name}</h3>
      <p className="card-ssh mono">
        {config.sshUser}@{config.sshHost}:{config.sshPort}
      </p>

      <div className="route-box">
        <div>
          <span>{strings.homeLocalLabel}</span>
          <MonoAddress color={color}>127.0.0.1:{config.localPort}</MonoAddress>
        </div>
        <Icon name="arrowRight" size={16} />
        <div>
          <span>{strings.homeRemoteLabel}</span>
          <MonoAddress>
            {config.remoteHost}:{config.remotePort}
          </MonoAddress>
        </div>
        {status.phase === "connected" ? (
          <button
            type="button"
            className="round-action"
            onClick={onOpenBrowser}
            title={strings.cardOpenBrowser}
            aria-label={strings.cardOpenBrowser}
          >
            <Icon name="external" size={15} />
          </button>
        ) : null}
      </div>

      <div className="card-chips">
        {config.jumps.length === 0 ? (
          <Chip icon="shield">{strings.cardDirect}</Chip>
        ) : (
          <Chip color="var(--secondary)" icon="route">
            {strings.cardJumps(config.jumps.length)}
          </Chip>
        )}
        {config.autoReconnect ? (
          <Chip color="var(--online)" icon="refresh">
            {strings.cardAutoReconnect}
          </Chip>
        ) : null}
        <Chip icon="speed">{strings.cardKeepalive(config.keepAliveSeconds)}</Chip>
      </div>

      {status.phase === "connecting" ? (
        <div className="connecting-row">
          <span className="spinner" />
          <span>{strings.cardPhaseConnecting}</span>
        </div>
      ) : null}

      {live && status.phase !== "connected" && status.phase !== "connecting" ? (
        <WarningBox
          danger={status.securityAlert}
          text={[errorText, status.nextRetrySeconds !== null
            ? strings.cardNextRetry(status.nextRetrySeconds)
            : null]
            .filter(Boolean)
            .join(" · ")}
        />
      ) : null}

      {status.phase === "error" && errorText ? (
        <WarningBox danger={status.securityAlert} text={errorText} />
      ) : null}

      {status.phase === "connected" ? (
        <p className="card-url mono">{config.browserOpenUrl}</p>
      ) : null}

      <div className="card-actions">
        {status.phase === "connected" ? (
          <>
            <button type="button" className="btn primary sm" onClick={onOpenBrowser}>
              <Icon name="external" size={16} />
              {strings.cardOpenBrowser}
            </button>
            <button type="button" className="btn outline sm" onClick={onDisconnect}>
              {strings.cardDisconnect}
            </button>
          </>
        ) : live ? (
          <button type="button" className="btn outline sm full" onClick={onDisconnect}>
            {strings.cardDisconnect}
          </button>
        ) : (
          <>
            <button
              type="button"
              className="btn primary sm"
              onClick={status.phase === "error" ? onRetry : onConnect}
            >
              <Icon name={status.phase === "error" ? "refresh" : "play"} size={16} />
              {status.phase === "error" ? strings.cardRetry : strings.cardConnect}
            </button>
            <button type="button" className="btn outline sm" onClick={onEdit}>
              <Icon name="edit" size={15} />
              {strings.cardEdit}
            </button>
          </>
        )}
      </div>
    </article>
  );
}
