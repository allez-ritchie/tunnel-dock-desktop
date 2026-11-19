import { useEffect, useRef, useState, type ReactNode } from "react";
import type { AppInput, AppView, GroupView, TunnelView } from "../api";
import { Icon, Modal, WarningBox } from "../components/ui";
import {
  appFormFrom,
  signature,
  toAppInput,
  validateApp,
  type AppForm,
  type FormErrors,
} from "../draft";
import { strings } from "../strings";

interface Props {
  apps: AppView[];
  tunnels: TunnelView[];
  groups: GroupView[];
  initial: AppView | null;
  defaults?: { groupId?: string; tunnelId?: string; localPort?: number };
  onClose: () => void;
  onSave: (input: AppInput) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onCreateTunnel: () => void;
}

export function AppEditorScreen({
  apps,
  tunnels,
  groups,
  initial,
  defaults,
  onClose,
  onSave,
  onDelete,
  onCreateTunnel,
}: Props) {
  const initialForm = useRef(appFormFrom(initial ?? undefined, defaults));
  const baseline = useRef(signature(initialForm.current));
  const [form, setForm] = useState<AppForm>(initialForm.current);
  const [errors, setErrors] = useState<FormErrors>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const editing = initial !== null;
  const dirty = signature(form) !== baseline.current;
  const selectedTunnel = tunnels.find((tunnel) => tunnel.id === form.tunnelId);
  const tunnelMissing = editing && initial !== null && !selectedTunnel;
  const duplicatePort = apps.find(
    (app) => app.id !== form.id && app.localPort === Number(form.localPort),
  );

  useEffect(() => {
    if (tunnels.length === 0) return;
    const preferred = defaults?.tunnelId ?? "";
    setForm((current) => {
      if (current.id !== null) return current;
      // A tunnel created from inside this editor comes back as `defaults`;
      // a stale reference (deleted tunnel) falls back to the first tunnel.
      if (preferred !== "" && current.tunnelId !== preferred) {
        return { ...current, tunnelId: preferred };
      }
      if (tunnels.some((tunnel) => tunnel.id === current.tunnelId)) return current;
      return { ...current, tunnelId: tunnels[0].id };
    });
  }, [defaults?.tunnelId, tunnels]);

  function patch(update: Partial<AppForm>) {
    setForm((current) => ({ ...current, ...update }));
  }

  function attemptClose() {
    if (dirty) setConfirmDiscard(true);
    else onClose();
  }

  async function save() {
    const nextErrors = validateApp(form, groups, tunnels);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;
    setSaving(true);
    setSaveError(null);
    try {
      await onSave(toAppInput(form));
    } catch (error) {
      setSaveError(strings.appEditorSaveFailed(String(error)));
      setSaving(false);
    }
  }

  async function remove() {
    if (!initial) return;
    setSaving(true);
    try {
      await onDelete(initial.id);
    } catch (error) {
      setSaveError(strings.appEditorSaveFailed(String(error)));
      setSaving(false);
    }
  }

  const localPreview = `http://127.0.0.1:${form.localPort || "?"}/  →  ${
    form.remoteHost || "?"
  }:${form.remotePort || "?"}`;
  const browserPreview = form.browserUrl.trim()
    ? form.browserUrl.replace("{port}", form.localPort || "?")
    : `http://127.0.0.1:${form.localPort || "?"}/`;

  return (
    <div className="screen editor-screen">
      <header className="editor-header">
        <button
          type="button"
          className="icon-btn"
          onClick={attemptClose}
          aria-label={strings.editorBack}
          title={strings.editorBack}
        >
          <Icon name="arrowLeft" size={19} />
        </button>
        <div className="editor-heading">
          <h1>{editing ? strings.appEditorEdit : strings.appEditorNew}</h1>
          <p className="mono">
            {selectedTunnel ? selectedTunnel.name : strings.appEditorNoTunnel}
          </p>
        </div>
      </header>

      <div className="editor-scroll">
        {tunnels.length === 0 ? (
          <WarningBox text={strings.appEditorNoTunnel} danger />
        ) : null}

        <FormCard title={strings.appEditorBasic}>
          <Field
            label={strings.appEditorName}
            value={form.name}
            placeholder={strings.appEditorNameHint}
            error={errors.name}
            onChange={(value) => {
              patch({ name: value });
              setErrors(({ name: _name, ...rest }) => rest);
            }}
          />

          <div className="field-row">
            <label
              className={`field${errors.groupId ? " has-error" : ""}`}
            >
              <span className="field-label">{strings.appEditorGroup}</span>
              <select
                value={form.groupId}
                onChange={(event) => {
                  patch({ groupId: event.target.value });
                  setErrors(({ groupId: _group, ...rest }) => rest);
                }}
              >
                {groups.map((group) => (
                  <option key={group.id} value={group.id}>
                    {group.name}
                  </option>
                ))}
              </select>
              {errors.groupId ? (
                <span className="field-error">{errors.groupId}</span>
              ) : (
                <span className="field-hint">{strings.appEditorGroupHint}</span>
              )}
            </label>

            <label
              className={`field${errors.tunnelId ? " has-error" : ""}`}
            >
              <span className="field-label">{strings.appEditorTunnel}</span>
              <select
                value={form.tunnelId}
                onChange={(event) => {
                  patch({ tunnelId: event.target.value });
                  setErrors(({ tunnelId: _tunnel, ...rest }) => rest);
                }}
              >
                {tunnelMissing ? (
                  <option value={form.tunnelId}>
                    {strings.appEditorTunnelMissing}
                  </option>
                ) : null}
                {tunnels.map((tunnel) => (
                  <option key={tunnel.id} value={tunnel.id}>
                    {tunnel.name} — {tunnel.endpoint}
                  </option>
                ))}
              </select>
              {errors.tunnelId ? (
                <span className="field-error">{errors.tunnelId}</span>
              ) : tunnelMissing ? (
                <span className="field-error">
                  {strings.appEditorTunnelMissing}
                </span>
              ) : (
                <span className="field-hint">{strings.appEditorTunnelHint}</span>
              )}
            </label>
          </div>

          {tunnelMissing || tunnels.length === 0 ? (
            <button
              type="button"
              className="btn outline sm"
              onClick={onCreateTunnel}
            >
              <Icon name="plus" size={16} />
              {strings.appEditorCreateTunnel}
            </button>
          ) : null}
        </FormCard>

        <FormCard title={strings.appEditorRouteSection}>
          <Field
            label={strings.appEditorLocalPort}
            hint={strings.appEditorLocalPortHint}
            value={form.localPort}
            inputMode="numeric"
            error={errors.localPort}
            onChange={(value) => {
              patch({ localPort: value.replace(/\D/g, "").slice(0, 5) });
              setErrors(({ localPort: _port, ...rest }) => rest);
            }}
          />
          {duplicatePort ? (
            <p className="field-hint warn">
              {strings.appEditorPortInUse(duplicatePort.localPort)}
            </p>
          ) : null}
          <div className="field-row">
            <Field
              label={strings.appEditorRemoteHost}
              placeholder={strings.appEditorRemoteHostHint}
              value={form.remoteHost}
              error={errors.remoteHost}
              onChange={(value) => {
                patch({ remoteHost: value });
                setErrors(({ remoteHost: _host, ...rest }) => rest);
              }}
            />
            <Field
              label={strings.appEditorRemotePort}
              value={form.remotePort}
              inputMode="numeric"
              error={errors.remotePort}
              onChange={(value) => {
                patch({ remotePort: value.replace(/\D/g, "").slice(0, 5) });
                setErrors(({ remotePort: _port, ...rest }) => rest);
              }}
            />
          </div>
          <code className="form-preview">{localPreview}</code>
        </FormCard>

        <FormCard title={strings.appEditorBrowserSection}>
          <Field
            label={strings.appEditorBrowserUrl}
            placeholder={strings.appEditorBrowserUrlHint}
            value={form.browserUrl}
            error={errors.browserUrl}
            onChange={(value) => {
              patch({ browserUrl: value });
              setErrors(({ browserUrl: _url, ...rest }) => rest);
            }}
          />
          <p className="field-hint">{strings.appEditorBrowserUrlHelp}</p>
          <code className="form-preview accent">{browserPreview}</code>
        </FormCard>

        <FormCard title={strings.appEditorAdvanced}>
          <SwitchRow
            title={strings.appEditorAutoBoot}
            subtitle={strings.appEditorAutoBootDesc}
            checked={form.autoStartOnBoot}
            onChange={(value) => patch({ autoStartOnBoot: value })}
          />
        </FormCard>

        {editing ? (
          <button
            type="button"
            className="btn outline danger-outline full"
            onClick={() => setConfirmDelete(true)}
          >
            <Icon name="trash" size={16} />
            {strings.appEditorDelete}
          </button>
        ) : null}
      </div>

      <footer className="editor-footer">
        {saveError ? <WarningBox text={saveError} danger /> : null}
        <div className="editor-footer-actions">
          <button type="button" className="btn ghost" onClick={attemptClose}>
            {strings.dialogCancel}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={saving || tunnels.length === 0}
            onClick={() => void save()}
          >
            {saving ? <span className="spinner light" /> : <Icon name="check" size={17} />}
            {saving ? strings.editorSaving : strings.editorSave}
          </button>
        </div>
      </footer>

      {confirmDiscard ? (
        <ConfirmInline
          title={strings.appEditorDiscardTitle}
          body={strings.appEditorDiscardBody}
          confirmLabel={strings.appEditorDiscardConfirm}
          onCancel={() => setConfirmDiscard(false)}
          onConfirm={() => {
            setConfirmDiscard(false);
            onClose();
          }}
        />
      ) : null}

      {confirmDelete && initial ? (
        <ConfirmInline
          title={strings.appDeleteTitle}
          body={strings.appDeleteBody(initial.name)}
          confirmLabel={strings.appDeleteConfirm}
          onCancel={() => setConfirmDelete(false)}
          onConfirm={() => {
            setConfirmDelete(false);
            void remove();
          }}
        />
      ) : null}
    </div>
  );
}

function ConfirmInline({
  title,
  body,
  confirmLabel,
  onCancel,
  onConfirm,
}: {
  title: string;
  body: string;
  confirmLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Modal title={title} onClose={onCancel} small>
      <p className="dialog-body">{body}</p>
      <div className="dialog-actions">
        <button type="button" className="btn ghost" onClick={onCancel}>
          {strings.dialogCancel}
        </button>
        <button type="button" className="btn danger" onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}

function FormCard({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <section className="form-card">
      <header>
        <h2>{title}</h2>
        {subtitle ? <p>{subtitle}</p> : null}
      </header>
      <div className="form-card-body">{children}</div>
    </section>
  );
}

function Field({
  label,
  hint,
  value,
  onChange,
  placeholder,
  error,
  type = "text",
  inputMode,
  multiline = false,
}: {
  label: string;
  hint?: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: string;
  type?: "text" | "password";
  inputMode?: "text" | "numeric";
  multiline?: boolean;
}) {
  return (
    <label className={`field${error ? " has-error" : ""}`}>
      <span className="field-label">{label}</span>
      {multiline ? (
        <textarea
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          rows={4}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <input
          type={type}
          value={value}
          placeholder={placeholder}
          inputMode={inputMode}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
      {error ? (
        <span className="field-error">{error}</span>
      ) : hint ? (
        <span className="field-hint">{hint}</span>
      ) : null}
    </label>
  );
}

function SwitchRow({
  title,
  subtitle,
  checked,
  onChange,
}: {
  title: string;
  subtitle: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="switch-row">
      <span>
        <strong>{title}</strong>
        <small>{subtitle}</small>
      </span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <i aria-hidden="true" />
    </label>
  );
}
