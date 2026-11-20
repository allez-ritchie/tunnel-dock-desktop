import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type BundlePreview, type ImportSummary } from "../api";
import { Icon, Modal, WarningBox } from "../components/ui";
import { strings } from "../strings";

interface Props {
  tunnelCount: number;
  appCount: number;
  groupCount: number;
  onMessage: (text: string) => void;
  /** 导入完成后刷新配置与计数。 */
  onImported: () => Promise<void>;
}

const ZIP_FILTER = [{ name: "TunnelDock 配置包", extensions: ["zip"] }];

/**
 * 配置包导出 / 导入。
 *
 * 导出的是“应用组 + 应用 + 隧道”的可移植清单；导入会按名称合并到现有配置，
 * 不会覆盖现有条目，也不会自动启动任何转发。
 */
export function BundleCard({
  tunnelCount,
  appCount,
  groupCount,
  onMessage,
  onImported,
}: Props) {
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [pending, setPending] = useState<{ path: string; preview: BundlePreview } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  const empty = tunnelCount === 0 && appCount === 0;

  async function doExport() {
    setError(null);
    try {
      const path = await save({
        title: strings.settingsBundleExportDialog,
        defaultPath: defaultBundleName(),
        filters: ZIP_FILTER,
      });
      if (!path) return;
      setExporting(true);
      const summary = await api.exportBundle(path);
      onMessage(
        strings.settingsBundleExported(summary.tunnels, summary.apps, summary.groups),
      );
      if (summary.passwordTunnels.length > 0) {
        onMessage(
          strings.settingsBundleExportSkipped(summary.passwordTunnels.join("、")),
        );
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setExporting(false);
    }
  }

  async function chooseImport() {
    setError(null);
    try {
      const path = await open({
        title: strings.settingsBundleImportDialog,
        multiple: false,
        directory: false,
        filters: ZIP_FILTER,
      });
      if (!path || Array.isArray(path)) return;
      setImporting(true);
      const preview = await api.previewBundle(path);
      setPending({ path, preview });
    } catch (failure) {
      setError(strings.bundleReadFailed(errorMessage(failure)));
    } finally {
      setImporting(false);
    }
  }

  async function confirmImport() {
    if (!pending) return;
    const path = pending.path;
    // 用预览里的统计判断“是否真的缺私钥”，避免无关提示。
    const missingKeys = pending.preview.tunnelsWithoutKey;
    setPending(null);
    setImporting(true);
    setError(null);
    try {
      const summary: ImportSummary = await api.importBundle(path);
      onMessage(
        strings.bundleImported(summary.tunnelsAdded, summary.appsAdded, summary.groupsAdded),
      );
      if (summary.appsSkipped > 0 || summary.appsDropped > 0) {
        onMessage(
          strings.bundleImportedSkipped(summary.appsSkipped, summary.appsDropped),
        );
      }
      if (summary.portConflicts.length > 0) {
        onMessage(strings.bundleImportedConflicts(summary.portConflicts.join("、")));
      }
      if (missingKeys > 0) {
        onMessage(strings.bundleImportedNoKey(missingKeys));
      }
      await onImported();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setImporting(false);
    }
  }

  return (
    <section className="settings-card">
      <header>
        <span className="settings-icon">
          <Icon name="package" size={20} />
        </span>
        <div>
          <h2>{strings.settingsBundle}</h2>
          <p>{strings.settingsBundleDesc}</p>
        </div>
      </header>
      <div className="settings-card-body">
        <p className="field-hint">
          {strings.settingsBundleCounts(tunnelCount, appCount, groupCount)}
        </p>
        <p className="field-hint subtle">{strings.settingsBundleNote}</p>
        <p className="field-hint subtle">{strings.settingsBundleFlexibleNote}</p>
        <div className="bundle-actions">
          <button
            type="button"
            className="btn outline sm"
            disabled={exporting || importing || empty}
            onClick={() => void doExport()}
          >
            {exporting ? (
              <span className="spinner" />
            ) : (
              <Icon name="download" size={16} />
            )}
            {exporting ? strings.settingsBundleExporting : strings.settingsBundleExport}
          </button>
          <button
            type="button"
            className="btn outline sm"
            disabled={exporting || importing}
            onClick={() => void chooseImport()}
          >
            {importing && !pending ? (
              <span className="spinner" />
            ) : (
              <Icon name="upload" size={16} />
            )}
            {importing && !pending ? strings.settingsBundleImporting : strings.settingsBundleImport}
          </button>
        </div>
        {empty ? <p className="field-hint">{strings.bundleEmpty}</p> : null}
        {error ? <WarningBox text={error} danger /> : null}
      </div>

      {pending ? (
        <Modal
          title={strings.bundlePreviewTitle}
          onClose={() => setPending(null)}
          small
        >
          <div className="modal-body">
            <p className="dialog-body">
              {strings.bundlePreviewBody(
                pending.preview.tunnelCount,
                pending.preview.appCount,
                pending.preview.groupCount,
              )}
            </p>
            {pending.preview.tunnelNames.length > 0 ? (
              <p className="bundle-name-list mono">
                {pending.preview.tunnelNames.join(" · ")}
              </p>
            ) : null}
            {pending.preview.groupNames.length > 0 ? (
              <p className="bundle-name-list mono">
                {pending.preview.groupNames.join(" · ")}
              </p>
            ) : null}
            {pending.preview.tunnelsWithoutKey > 0 ? (
              <WarningBox
                text={strings.bundlePreviewMissingKeys(pending.preview.tunnelsWithoutKey)}
              />
            ) : null}
            <p className="field-hint">{strings.bundlePreviewConflictHint}</p>
          </div>
          <div className="dialog-actions">
            <button
              type="button"
              className="btn ghost"
              onClick={() => setPending(null)}
            >
              {strings.dialogCancel}
            </button>
            <button
              type="button"
              className="btn primary"
              onClick={() => void confirmImport()}
            >
              <Icon name="upload" size={16} />
              {strings.bundlePreviewConfirm}
            </button>
          </div>
        </Modal>
      ) : null}
    </section>
  );
}

function defaultBundleName(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `tunneldock-${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(
    now.getDate(),
  )}-${pad(now.getHours())}${pad(now.getMinutes())}.zip`;
}
