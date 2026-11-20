import { useEffect, useMemo, useRef, useState } from "react";
import { api, type LocalSshStatus, type SshConfigHost } from "../api";
import { Icon, WarningBox } from "../components/ui";
import { strings } from "../strings";

interface Props {
  /** 已存在的隧道名称，用于提示“将跳过”。 */
  existingNames: string[];
  onClose: () => void;
  /** 执行导入，成功后由外壳刷新配置并返回。 */
  onImport: (aliases: string[]) => Promise<number>;
}

export function SshConfigImportScreen({ existingNames, onClose, onImport }: Props) {
  const [status, setStatus] = useState<LocalSshStatus | null>(null);
  const [hosts, setHosts] = useState<SshConfigHost[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  // 上游每次渲染都会传一个新数组，用 ref 固定首次的值，避免 effect 反复运行。
  const knownNames = useRef(existingNames);

  useEffect(() => {
    let disposed = false;
    void (async () => {
      try {
        const [sshStatus, configHosts] = await Promise.all([
          api.localSshStatus(),
          api.localSshConfigHosts(),
        ]);
        if (disposed) return;
        setStatus(sshStatus);
        setHosts(configHosts);
        // 默认勾选所有可导入、且尚未导入过的条目。
        setSelected(
          new Set(
            configHosts
              .filter((host) => host.importable)
              .filter(
                (host) =>
                  !knownNames.current.some(
                    (name) => name.toLowerCase() === host.alias.toLowerCase(),
                  ),
              )
              .map((host) => host.alias),
          ),
        );
      } catch (failure) {
        if (!disposed) setError(String(failure));
      } finally {
        if (!disposed) setLoading(false);
      }
    })();
    return () => {
      disposed = true;
    };
  }, []);

  const importable = useMemo(
    () => hosts.filter((host) => host.importable),
    [hosts],
  );
  const selectedImportable = importable.filter((host) => selected.has(host.alias));

  function toggle(alias: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(alias)) next.delete(alias);
      else next.add(alias);
      return next;
    });
  }

  async function runImport() {
    if (selectedImportable.length === 0) {
      setError(strings.sshConfigSelectedNone);
      return;
    }
    setImporting(true);
    setError(null);
    try {
      const count = await onImport(selectedImportable.map((host) => host.alias));
      if (count === 0) {
        setError(strings.sshConfigAlreadyExists);
        setImporting(false);
        return;
      }
      onClose();
    } catch (failure) {
      setError(String(failure));
      setImporting(false);
    }
  }

  return (
    <div className="screen editor-screen">
      <header className="editor-header">
        <button
          type="button"
          className="icon-btn"
          onClick={onClose}
          aria-label={strings.sshConfigBack}
          title={strings.sshConfigBack}
        >
          <Icon name="arrowLeft" size={19} />
        </button>
        <div className="editor-heading">
          <h1>{strings.sshConfigTitle}</h1>
          <p className="mono">{status?.configPath ?? "~/.ssh/config"}</p>
        </div>
      </header>

      <div className="editor-scroll">
        {loading ? (
          <p className="muted-row">
            <span className="spinner" />
            {strings.localKeyLoading}
          </p>
        ) : !status?.configAvailable ? (
          <section className="empty-state">
            <Icon name="terminal" size={42} />
            <h2>{strings.sshConfigMissing}</h2>
            <p>{strings.sshConfigMissingHint}</p>
          </section>
        ) : hosts.length === 0 ? (
          <section className="empty-state">
            <Icon name="route" size={42} />
            <h2>{strings.sshConfigEmpty}</h2>
          </section>
        ) : (
          <>
            <section className="config-summary">
              <div>
                <strong>{importable.length}</strong>
                <span>{strings.sshConfigImportableLabel}</span>
              </div>
              <div>
                <strong>{hosts.length - importable.length}</strong>
                <span>{strings.sshConfigUnsupportedLabel}</span>
              </div>
              <div>
                <strong>{selectedImportable.length}</strong>
                <span>{strings.sshConfigSelectedLabel}</span>
              </div>
            </section>

            <p className="field-hint">
              {strings.sshConfigFound(hosts.length, importable.length)}
            </p>

            <div className="config-toolbar">
              <button
                type="button"
                className="btn ghost sm"
                onClick={() =>
                  setSelected(new Set(importable.map((host) => host.alias)))
                }
              >
                <Icon name="check" size={15} />
                {strings.sshConfigSelectAll}
              </button>
              <button
                type="button"
                className="btn ghost sm"
                onClick={() => setSelected(new Set())}
              >
                <Icon name="close" size={15} />
                {strings.sshConfigClear}
              </button>
            </div>

            <div className="config-list">
              {hosts.map((host) => {
                const duplicate = existingNames.some(
                  (name) => name.toLowerCase() === host.alias.toLowerCase(),
                );
                const key = localKeyLabel(host, status);
                return (
                  <label
                    key={host.alias}
                    className={`config-row${host.importable ? "" : " is-disabled"}`}
                  >
                    <input
                      type="checkbox"
                      checked={selected.has(host.alias)}
                      disabled={!host.importable}
                      onChange={() => toggle(host.alias)}
                    />
                    <div className="config-row-main">
                      <div className="config-row-head">
                        <strong>{host.alias}</strong>
                        <span className="mono">
                          {host.user}@{host.hostName}:{host.port}
                        </span>
                      </div>
                      <div className="config-row-meta">
                        {duplicate ? (
                          <span className="config-badge is-muted">
                            {strings.sshConfigAlreadyExists}
                          </span>
                        ) : null}
                        {key ? (
                          <span className="config-badge" title={host.identityFile ?? ""}>
                            <Icon name="key" size={12} />
                            {key}
                          </span>
                        ) : (
                          <span className="config-badge is-warn">
                            <Icon name="alert" size={12} />
                            {strings.sshConfigNoKey}
                          </span>
                        )}
                        {host.proxyJump.length > 0 ? (
                          <span className="config-badge">
                            <Icon name="route" size={12} />
                            {strings.sshConfigJumpChain(host.proxyJump.length)}
                          </span>
                        ) : null}
                        {host.importable ? null : (
                          <span className="config-badge is-warn">
                            {strings.sshConfigUnsupported(unsupportedLabel(host))}
                          </span>
                        )}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          </>
        )}

        {error ? <WarningBox text={error} danger /> : null}
      </div>

      <footer className="editor-footer">
        <p className="field-hint">{strings.sshConfigFooterHint}</p>
        <div className="editor-footer-actions">
          <button type="button" className="btn ghost" onClick={onClose}>
            {strings.dialogCancel}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={importing || selectedImportable.length === 0}
            onClick={() => void runImport()}
          >
            {importing ? (
              <span className="spinner light" />
            ) : (
              <Icon name="download" size={16} />
            )}
            {importing ? strings.sshConfigImporting : strings.sshConfigImport}
          </button>
        </div>
      </footer>
    </div>
  );
}

function unsupportedLabel(host: SshConfigHost): string {
  switch (host.unsupported) {
    case "proxy_command":
      return strings.sshConfigUnsupportedProxyCommand;
    case "include":
      return strings.sshConfigUnsupportedInclude;
    case "match":
      return strings.sshConfigUnsupportedMatch;
    case "proxy_jump":
      return strings.sshConfigUnsupportedProxyJump;
    default:
      return host.unsupported ?? "";
  }
}

/** 把 `IdentityFile` 精简成 `~/.ssh/xxx` 里的文件名，找不到时返回空串。 */
function localKeyLabel(host: SshConfigHost, status: LocalSshStatus | null): string {
  const raw = host.identityFile;
  if (!raw || !status) return "";
  const name = raw.split(/[\\/]/).pop() ?? "";
  if (!name) return "";
  const known = status.keys.some((key) => key.name === name);
  return known ? strings.sshConfigKeyFound(name) : "";
}
