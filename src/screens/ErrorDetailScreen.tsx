import type { AppStatus, AppView, GroupView, TunnelView } from "../api";
import { Chip, Icon, MonoAddress } from "../components/ui";
import { describeError, strings } from "../strings";

interface Props {
  app: AppView | undefined;
  tunnel: TunnelView | undefined;
  group: GroupView | undefined;
  status: AppStatus | undefined;
  onBack: () => void;
  onRetry: () => void;
  onEdit: () => void;
}

export function ErrorDetailScreen({
  app,
  tunnel,
  group,
  status,
  onBack,
  onRetry,
  onEdit,
}: Props) {
  if (!app || !status) {
    return (
      <div className="screen editor-screen error-detail-screen">
        <header className="editor-header">
          <button
            type="button"
            className="icon-btn"
            onClick={onBack}
            aria-label={strings.editorBack}
            title={strings.editorBack}
          >
            <Icon name="arrowLeft" size={19} />
          </button>
          <div className="editor-heading">
            <h1>{strings.errorDetailTitle}</h1>
          </div>
        </header>
        <div className="error-detail-body">
          <section className="empty-state">
            <Icon name="alert" size={44} />
            <h2>{strings.errorDetailMissingApp}</h2>
            <button type="button" className="btn primary" onClick={onBack}>
              {strings.errorDetailBack}
            </button>
          </section>
        </div>
      </div>
    );
  }

  const errorText = describeError(
    status.errorCode,
    status.errorDetail,
    app.localPort,
  );
  const occurredAt = status.updatedAt
    ? new Date(status.updatedAt).toLocaleString()
    : "—";

  return (
    <div className="screen editor-screen error-detail-screen">
      <header className="editor-header">
        <button
          type="button"
          className="icon-btn"
          onClick={onBack}
          aria-label={strings.editorBack}
          title={strings.editorBack}
        >
          <Icon name="arrowLeft" size={19} />
        </button>
        <div className="editor-heading">
          <h1>{strings.errorDetailTitle}</h1>
          <p className="mono">{app.name}</p>
        </div>
      </header>

      <div className="error-detail-body">
        <section className="error-hero">
          <span className="error-hero-icon">
            <Icon name="alert" size={30} />
          </span>
          <div>
            <span className="error-hero-kicker">
              {strings.errorDetailUnavailable}
            </span>
            <h2>{app.name}</h2>
            <p>{errorText}</p>
          </div>
          {status.securityAlert ? (
            <Chip color="var(--danger)" icon="shield">
              {strings.tunnelTestSecurity}
            </Chip>
          ) : null}
        </section>

        <section className="error-facts">
          <Fact label={strings.errorDetailReason} value={errorText} wide />
          <Fact
            label={strings.errorDetailCode}
            value={status.errorCode ?? "—"}
            mono
          />
          <Fact label={strings.errorDetailTime} value={occurredAt} />
          <Fact
            label={strings.appEditorGroup}
            value={group?.name ?? "—"}
            wide
          />
          <Fact
            label={strings.errorDetailTunnel}
            value={tunnel ? tunnel.name : strings.errorDetailTunnelMissing}
            wide
          />
          {tunnel ? (
            <Fact
              label={strings.tunnelCardEndpoint}
              value={tunnel.endpoint}
              mono
              wide
            />
          ) : null}
          {tunnel ? (
            <Fact
              label={strings.tunnelCardChain}
              value={tunnel.chainSummary}
              mono
              wide
            />
          ) : null}
          <Fact
            label={strings.errorDetailRoute}
            value={`127.0.0.1:${app.localPort} → ${app.remoteHost}:${app.remotePort}`}
            mono
            wide
          />
        </section>

        <section className="error-technical">
          <h3>{strings.errorDetailTechnical}</h3>
          <pre>{status.errorDetail ?? strings.errorDetailNoDetail}</pre>
        </section>

        <p className="field-hint">{strings.errorDetailBlockedHint}</p>
      </div>

      <footer className="editor-footer">
        <div className="editor-footer-actions">
          <button type="button" className="btn ghost" onClick={onBack}>
            {strings.errorDetailBack}
          </button>
          <button type="button" className="btn outline sm" onClick={onEdit}>
            <Icon name="edit" size={15} />
            {strings.errorDetailEdit}
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={onRetry}
            disabled={!tunnel}
          >
            <Icon name="refresh" size={16} />
            {strings.errorDetailRetry}
          </button>
        </div>
      </footer>
    </div>
  );
}

function Fact({
  label,
  value,
  mono = false,
  wide = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
  wide?: boolean;
}) {
  return (
    <div className={`error-fact${wide ? " is-wide" : ""}`}>
      <span>{label}</span>
      {mono ? <MonoAddress>{value}</MonoAddress> : <strong>{value}</strong>}
    </div>
  );
}
