import { useRef, useState, type ReactNode } from "react";
import type { AppView, AuthType, TunnelInput, TunnelView } from "../api";
import { Icon, Modal, SecureNotice, WarningBox } from "../components/ui";
import { LocalKeyPicker } from "../components/LocalKeyPicker";
import {
  MAX_ENDPOINTS,
  MAX_JUMPS,
  chainPreview,
  duplicateEndpointLabel,
  endpointChainPreview,
  endpointError,
  endpointFormFrom,
  hasSavedPassphrase,
  hasSavedPassword,
  hasSavedPrivateKey,
  jumpError,
  jumpFormFrom,
  nextEndpointLabel,
  signature,
  toTunnelInput,
  tunnelFormFrom,
  usesInheritedCredentials,
  usesLocalKey,
  validateTunnel,
  type EndpointForm,
  type FormErrors,
  type JumpForm,
  type TunnelForm,
} from "../draft";
import { strings } from "../strings";

interface Props {
  initial: TunnelView | null;
  usedBy: AppView[];
  onClose: () => void;
  onSave: (input: TunnelInput) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

export function TunnelEditorScreen({ initial, usedBy, onClose, onSave, onDelete }: Props) {
  const initialForm = useRef(tunnelFormFrom(initial ?? undefined));
  const baseline = useRef(signature(initialForm.current));
  const [form, setForm] = useState<TunnelForm>(initialForm.current);
  const [errors, setErrors] = useState<FormErrors>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [confirmSave, setConfirmSave] = useState(false);
  const [endpointEditor, setEndpointEditor] = useState<{
    index: number;
    endpoint: EndpointForm;
  } | null>(null);

  const editing = initial !== null;
  const dirty = signature(form) !== baseline.current;
  const usedCount = editing ? usedBy.length : 0;

  function patch(update: Partial<TunnelForm>) {
    setForm((current) => ({ ...current, ...update }));
  }

  function clearEndpointsError() {
    setErrors(({ endpoints: _endpoints, ...rest }) => rest);
  }

  function attemptClose() {
    if (dirty) setConfirmDiscard(true);
    else onClose();
  }

  async function doSave() {
    setSaving(true);
    setSaveError(null);
    try {
      await onSave(toTunnelInput(form));
    } catch (error) {
      setSaveError(strings.editorSaveFailed(String(error)));
      setSaving(false);
    }
  }

  async function save() {
    const nextErrors = validateTunnel(form);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;
    if (editing && usedCount > 0) {
      setConfirmSave(true);
      return;
    }
    await doSave();
  }

  async function remove() {
    if (!initial) return;
    setSaving(true);
    try {
      await onDelete(initial.id);
    } catch (error) {
      setSaveError(strings.editorSaveFailed(String(error)));
      setSaving(false);
    }
  }

  /**
   * 删掉一条线路。首选线路被删时回落到剩下第一条，否则界面会一直指向一条
   * 已经不存在的线路。
   */
  function removeEndpoint(index: number) {
    const removed = form.endpoints[index];
    const endpoints = form.endpoints.filter((_, current) => current !== index);
    patch({
      endpoints,
      preferredEndpointId:
        form.preferredEndpointId !== null &&
        removed.id !== null &&
        form.preferredEndpointId === removed.id
          ? (endpoints[0]?.id ?? null)
          : form.preferredEndpointId,
    });
    clearEndpointsError();
  }

  function moveEndpoint(from: number, to: number) {
    if (to < 0 || to >= form.endpoints.length) return;
    const endpoints = [...form.endpoints];
    [endpoints[from], endpoints[to]] = [endpoints[to], endpoints[from]];
    patch({ endpoints });
  }

  function togglePreferred(endpoint: EndpointForm) {
    if (endpoint.id === null) return;
    // 再点一次取消钉住，回到“自动选择”。
    patch({
      preferredEndpointId:
        form.preferredEndpointId === endpoint.id ? null : endpoint.id,
    });
  }

  const preferredLabel =
    form.endpoints.find(
      (endpoint) => endpoint.id !== null && endpoint.id === form.preferredEndpointId,
    )?.label ?? "";

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
          <h1>{editing ? strings.editorEditTunnel : strings.editorNewTunnel}</h1>
          <p className="mono">{chainPreview(form)}</p>
        </div>
      </header>

      <div className="editor-scroll">
        {usedCount > 0 ? (
          <WarningBox text={strings.editorTunnelUsingHint(usedCount)} />
        ) : null}

        <FormCard title={strings.editorBasic}>
          <Field
            label={strings.editorName}
            value={form.name}
            placeholder={strings.editorNameHint}
            error={errors.name}
            onChange={(value) => {
              patch({ name: value });
              setErrors(({ name: _name, ...rest }) => rest);
            }}
          />
          <code className="form-preview">{chainPreview(form)}</code>
        </FormCard>

        <FormCard
          title={strings.endpointSection}
          subtitle={strings.endpointSectionDesc}
        >
          {form.endpoints.length === 0 ? (
            <WarningBox text={strings.endpointEmpty} />
          ) : (
            <div className="endpoint-list">
              {form.endpoints.map((endpoint, index) => (
                <EndpointRow
                  key={endpoint.id ?? `new-${index}`}
                  endpoint={endpoint}
                  index={index}
                  preferred={
                    endpoint.id !== null && endpoint.id === form.preferredEndpointId
                  }
                  duplicateLabel={duplicateEndpointLabel(form.endpoints, index)}
                  onEdit={() => setEndpointEditor({ index, endpoint })}
                  onTogglePreferred={() => togglePreferred(endpoint)}
                  onMoveUp={index > 0 ? () => moveEndpoint(index, index - 1) : null}
                  onMoveDown={
                    index < form.endpoints.length - 1
                      ? () => moveEndpoint(index, index + 1)
                      : null
                  }
                  onRemove={() => removeEndpoint(index)}
                />
              ))}
            </div>
          )}

          {form.endpoints.length > 0 ? (
            <p className="field-hint">
              {strings.endpointCount(form.endpoints.length)}
              {" · "}
              {form.preferredEndpointId === null
                ? strings.endpointPreferredAuto
                : strings.endpointPreferredOn(preferredLabel)}
              {" · "}
              {strings.endpointPreferredAutoHint}
            </p>
          ) : null}

          {errors.endpoints ? (
            <WarningBox text={errors.endpoints} danger />
          ) : null}

          <button
            type="button"
            className="btn outline sm"
            disabled={form.endpoints.length >= MAX_ENDPOINTS}
            onClick={() =>
              setEndpointEditor({
                index: -1,
                endpoint: endpointFormFrom(
                  undefined,
                  nextEndpointLabel(form.endpoints.map((item) => item.label)),
                ),
              })
            }
          >
            <Icon name="plus" size={16} />
            {strings.endpointAdd}
          </button>
        </FormCard>

        <FormCard title={strings.editorAdvanced}>
          <SwitchRow
            title={strings.editorAutoReconnect}
            subtitle={strings.editorAutoReconnectDesc}
            checked={form.autoReconnect}
            onChange={(value) => patch({ autoReconnect: value })}
          />
          <SwitchRow
            title={strings.editorStrictHost}
            subtitle={strings.editorStrictHostDesc}
            checked={form.strictHostKey}
            onChange={(value) => patch({ strictHostKey: value })}
          />
          <div className="field-row">
            <Field
              label={strings.editorKeepalive}
              hint={strings.editorKeepaliveDesc}
              value={form.keepAliveSeconds}
              inputMode="numeric"
              error={errors.keepAliveSeconds}
              onChange={(value) => {
                patch({ keepAliveSeconds: value.replace(/\D/g, "").slice(0, 3) });
                setErrors(({ keepAliveSeconds: _value, ...rest }) => rest);
              }}
            />
            <Field
              label={strings.editorTimeout}
              hint={strings.editorTimeoutDesc}
              value={form.connectTimeoutSeconds}
              inputMode="numeric"
              error={errors.connectTimeoutSeconds}
              onChange={(value) => {
                patch({
                  connectTimeoutSeconds: value.replace(/\D/g, "").slice(0, 3),
                });
                setErrors(({ connectTimeoutSeconds: _value, ...rest }) => rest);
              }}
            />
          </div>
        </FormCard>

        {editing ? (
          <button
            type="button"
            className="btn outline danger-outline full"
            onClick={() => setConfirmDelete(true)}
          >
            <Icon name="trash" size={16} />
            {strings.editorDeleteTunnel}
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
            disabled={saving}
            onClick={() => void save()}
          >
            {saving ? <span className="spinner light" /> : <Icon name="check" size={17} />}
            {saving ? strings.editorSaving : strings.editorSave}
          </button>
        </div>
      </footer>

      {endpointEditor ? (
        <EndpointDialog
          initial={endpointEditor.endpoint}
          otherLabels={form.endpoints
            .filter((_, index) => index !== endpointEditor.index)
            .map((endpoint) => endpoint.label)}
          onClose={() => setEndpointEditor(null)}
          onSave={(endpoint) => {
            const next = [...form.endpoints];
            if (endpointEditor.index >= 0) next[endpointEditor.index] = endpoint;
            else next.push(endpoint);
            patch({
              endpoints: next,
              // 第一条线路自动成为首选，卡片才有东西可展示。
              preferredEndpointId:
                form.preferredEndpointId ??
                (next.length === 1 ? next[0].id : form.preferredEndpointId),
            });
            clearEndpointsError();
            setEndpointEditor(null);
          }}
        />
      ) : null}

      {confirmDiscard ? (
        <ConfirmInline
          title={strings.editorDiscardTitle}
          body={strings.editorDiscardBody}
          confirmLabel={strings.editorDiscardConfirm}
          onCancel={() => setConfirmDiscard(false)}
          onConfirm={() => {
            setConfirmDiscard(false);
            onClose();
          }}
        />
      ) : null}

      {confirmSave ? (
        <ConfirmInline
          title={strings.tunnelEditConfirmTitle}
          body={strings.tunnelEditConfirmBody(form.name.trim(), usedCount)}
          confirmLabel={strings.tunnelEditConfirm}
          onCancel={() => setConfirmSave(false)}
          onConfirm={() => {
            setConfirmSave(false);
            void doSave();
          }}
        />
      ) : null}

      {confirmDelete && initial ? (
        <ConfirmInline
          title={strings.tunnelDeleteTitle}
          body={
            usedCount > 0
              ? strings.tunnelDeleteBody(initial.name, usedCount)
              : strings.tunnelDeleteBodyUnused(initial.name)
          }
          confirmLabel={strings.tunnelDeleteConfirm}
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

/** 隧道编辑器里的一行线路：顺序、首选、徽标与编辑入口。 */
function EndpointRow({
  endpoint,
  index,
  preferred,
  duplicateLabel,
  onEdit,
  onTogglePreferred,
  onMoveUp,
  onMoveDown,
  onRemove,
}: {
  endpoint: EndpointForm;
  index: number;
  preferred: boolean;
  duplicateLabel: boolean;
  onEdit: () => void;
  onTogglePreferred: () => void;
  onMoveUp: (() => void) | null;
  onMoveDown: (() => void) | null;
  onRemove: () => void;
}) {
  return (
    <div
      className={[
        "endpoint-row",
        preferred ? "is-preferred" : "",
        endpoint.enabled ? "" : "is-disabled",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <span className="endpoint-index">{index + 1}</span>
      <button type="button" className="endpoint-main" onClick={onEdit}>
        <strong>{endpoint.label || endpoint.host || "?"}</strong>
        <span className="mono">
          {endpoint.username}@{endpoint.host}:{endpoint.port}
        </span>
        <span className="endpoint-chain">
          {endpoint.jumps.length === 0
            ? strings.endpointDirect
            : strings.endpointJumpCount(endpoint.jumps.length)}
          {" · "}
          {endpointChainPreview(endpoint)}
        </span>
        {endpoint.wifiOnly ? (
          <em className="endpoint-badge">{strings.endpointWifiOnlyBadge}</em>
        ) : null}
        {endpoint.enabled ? null : (
          <em className="endpoint-badge is-danger">
            {strings.endpointDisabledBadge}
          </em>
        )}
        {duplicateLabel ? (
          <em className="endpoint-badge is-danger">
            {strings.errorEndpointLabelDuplicate}
          </em>
        ) : null}
      </button>
      <div className="endpoint-tools">
        <button
          type="button"
          className={`icon-btn sm${preferred ? " is-active" : ""}`}
          title={strings.endpointPreferred}
          aria-label={strings.endpointPreferred}
          aria-pressed={preferred}
          onClick={onTogglePreferred}
        >
          <Icon name="star" size={16} />
        </button>
        <button
          type="button"
          className="icon-btn sm"
          disabled={onMoveUp === null}
          title={strings.endpointMoveUp}
          aria-label={strings.endpointMoveUp}
          onClick={onMoveUp ?? undefined}
        >
          <Icon name="chevronUp" size={16} />
        </button>
        <button
          type="button"
          className="icon-btn sm"
          disabled={onMoveDown === null}
          title={strings.endpointMoveDown}
          aria-label={strings.endpointMoveDown}
          onClick={onMoveDown ?? undefined}
        >
          <Icon name="chevronDown" size={16} />
        </button>
        <button
          type="button"
          className="icon-btn sm"
          title={strings.endpointEdit}
          aria-label={strings.endpointEdit}
          onClick={onEdit}
        >
          <Icon name="edit" size={15} />
        </button>
        <button
          type="button"
          className="icon-btn sm danger-icon"
          title={strings.endpointDelete}
          aria-label={strings.endpointDelete}
          onClick={onRemove}
        >
          <Icon name="trash" size={15} />
        </button>
      </div>
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

export function Field({
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

export function Segmented({
  value,
  onChange,
  options,
}: {
  value: AuthType;
  onChange: (value: AuthType) => void;
  options: { value: AuthType; label: string; icon: "key" | "lock" }[];
}) {
  return (
    <div className="segmented">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={value === option.value ? "is-active" : ""}
          onClick={() => onChange(option.value)}
        >
          <Icon name={option.icon} size={15} />
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function SwitchRow({
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

/**
 * 单条线路的编辑对话框：地址、凭据、跳板链与“仅局域网 / 启用”开关。
 *
 * 顺序与首选由隧道编辑器负责，这里只编辑一条线路自身，避免对话框里再套一层
 * 排序操作。
 */
function EndpointDialog({
  initial,
  otherLabels,
  onClose,
  onSave,
}: {
  initial: EndpointForm;
  otherLabels: string[];
  onClose: () => void;
  onSave: (endpoint: EndpointForm) => void;
}) {
  const [endpoint, setEndpoint] = useState<EndpointForm>(initial);
  const [error, setError] = useState<string | null>(null);
  const [jumpEditor, setJumpEditor] = useState<{
    index: number;
    jump: JumpForm;
  } | null>(null);
  const isNew = initial.host === "" && initial.id === null;

  function patch(update: Partial<EndpointForm>) {
    setEndpoint((current) => ({ ...current, ...update }));
    setError(null);
  }

  function checkAndSave() {
    const label = endpoint.label.trim();
    if (label !== "" && otherLabels.some((item) => item.trim() === label)) {
      setError(strings.errorEndpointLabelDuplicate);
      return;
    }
    const validation = endpointError(endpoint);
    if (validation !== null) {
      setError(validation);
      return;
    }
    onSave(endpoint);
  }

  return (
    <Modal
      title={isNew ? strings.endpointAddTitle : strings.endpointEditTitle}
      onClose={onClose}
    >
      <div className="modal-body">
        <Field
          label={strings.endpointLabel}
          placeholder={strings.endpointLabelHint}
          value={endpoint.label}
          onChange={(value) => patch({ label: value })}
        />
        <div className="field-row">
          <Field
            label={strings.endpointHost}
            placeholder={strings.endpointHostHint}
            value={endpoint.host}
            onChange={(value) => patch({ host: value })}
          />
          <Field
            label={strings.endpointPort}
            value={endpoint.port}
            inputMode="numeric"
            onChange={(value) =>
              patch({ port: value.replace(/\D/g, "").slice(0, 5) })
            }
          />
        </div>
        <Field
          label={strings.endpointUsername}
          value={endpoint.username}
          onChange={(value) => patch({ username: value })}
        />

        <Segmented
          value={endpoint.authType}
          onChange={(value) => patch({ authType: value })}
          options={[
            { value: "password", label: strings.editorAuthPassword, icon: "key" },
            { value: "private_key", label: strings.editorAuthKey, icon: "lock" },
          ]}
        />

        {endpoint.authType === "password" ? (
          <div className="secret-block">
            <Field
              label={
                hasSavedPassword(endpoint)
                  ? strings.editorPasswordSaved
                  : strings.editorPassword
              }
              placeholder={
                hasSavedPassword(endpoint)
                  ? strings.editorSecretKeepHint
                  : undefined
              }
              value={endpoint.passwordInput}
              type="password"
              onChange={(value) =>
                patch({ passwordInput: value, passwordCleared: false })
              }
            />
            {hasSavedPassword(endpoint) ? (
              <button
                type="button"
                className="link-danger"
                onClick={() =>
                  patch({ passwordInput: "", passwordCleared: true })
                }
              >
                {strings.editorClearSecret}
              </button>
            ) : null}
          </div>
        ) : (
          <div className="secret-block">
            <LocalKeyPicker
              value={endpoint.privateKeyPath}
              hasSavedKey={hasSavedPrivateKey(endpoint)}
              onChange={(name) => patch({ privateKeyPath: name })}
            />
            {usesLocalKey(endpoint) ? null : (
              <Field
                label={
                  hasSavedPrivateKey(endpoint)
                    ? strings.editorPrivateKeySaved
                    : strings.editorPrivateKey
                }
                placeholder={
                  hasSavedPrivateKey(endpoint)
                    ? strings.editorSecretKeepHint
                    : strings.editorPrivateKeyHint
                }
                value={endpoint.privateKeyInput}
                multiline
                onChange={(value) =>
                  patch({ privateKeyInput: value, privateKeyCleared: false })
                }
              />
            )}
            <Field
              label={
                hasSavedPassphrase(endpoint)
                  ? `${strings.editorPassphrase} · ${strings.editorPasswordSaved}`
                  : strings.editorPassphrase
              }
              placeholder={
                hasSavedPassphrase(endpoint)
                  ? strings.editorSecretKeepHint
                  : undefined
              }
              value={endpoint.passphraseInput}
              type="password"
              onChange={(value) =>
                patch({ passphraseInput: value, passphraseCleared: false })
              }
            />
            {hasSavedPrivateKey(endpoint) ? (
              <button
                type="button"
                className="link-danger"
                onClick={() =>
                  patch({
                    privateKeyInput: "",
                    privateKeyCleared: true,
                    privateKeyPath: null,
                    passphraseInput: "",
                    passphraseCleared: true,
                  })
                }
              >
                {strings.editorClearSecret}
              </button>
            ) : null}
          </div>
        )}

        <div className="jump-list">
          <h3 className="form-subtitle">{strings.editorJumpSection}</h3>
          {endpoint.jumps.length === 0 ? (
            <p className="muted-row">
              <Icon name="shield" size={16} />
              {strings.editorDirect}
            </p>
          ) : (
            endpoint.jumps.map((jump, index) => (
              <div className="jump-row" key={jump.id ?? `new-${index}`}>
                <span className="jump-index">{index + 1}</span>
                <button
                  type="button"
                  className="jump-main"
                  onClick={() => setJumpEditor({ index, jump })}
                >
                  <strong>{jump.label || jump.host || "?"}</strong>
                  <span className="mono">
                    {jump.username}@{jump.host}:{jump.port}
                  </span>
                  {usesInheritedCredentials(jump) ? (
                    <em className="jump-inherited">
                      {strings.jumpInherited(jump.inheritedFrom ?? "")}
                    </em>
                  ) : null}
                </button>
                <div className="jump-tools">
                  <button
                    type="button"
                    className="icon-btn sm"
                    disabled={index === 0}
                    title={strings.editorJumpMoveUp}
                    aria-label={strings.editorJumpMoveUp}
                    onClick={() => {
                      const jumps = [...endpoint.jumps];
                      [jumps[index - 1], jumps[index]] = [
                        jumps[index],
                        jumps[index - 1],
                      ];
                      patch({ jumps });
                    }}
                  >
                    <Icon name="chevronUp" size={16} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn sm"
                    disabled={index === endpoint.jumps.length - 1}
                    title={strings.editorJumpMoveDown}
                    aria-label={strings.editorJumpMoveDown}
                    onClick={() => {
                      const jumps = [...endpoint.jumps];
                      [jumps[index + 1], jumps[index]] = [
                        jumps[index],
                        jumps[index + 1],
                      ];
                      patch({ jumps });
                    }}
                  >
                    <Icon name="chevronDown" size={16} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn sm"
                    title={strings.editorJumpEdit}
                    aria-label={strings.editorJumpEdit}
                    onClick={() => setJumpEditor({ index, jump })}
                  >
                    <Icon name="edit" size={15} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn sm danger-icon"
                    title={strings.editorJumpDelete}
                    aria-label={strings.editorJumpDelete}
                    onClick={() =>
                      patch({
                        jumps: endpoint.jumps.filter((_, i) => i !== index),
                      })
                    }
                  >
                    <Icon name="trash" size={15} />
                  </button>
                </div>
              </div>
            ))
          )}
          <button
            type="button"
            className="btn outline sm"
            disabled={endpoint.jumps.length >= MAX_JUMPS}
            onClick={() =>
              setJumpEditor({ index: -1, jump: jumpFormFrom() })
            }
          >
            <Icon name="plus" size={16} />
            {strings.editorAddJump}
          </button>
          <p className="field-hint">{strings.editorJumpHelp}</p>
        </div>

        <SwitchRow
          title={strings.endpointEnabled}
          subtitle={strings.endpointEnabledDesc}
          checked={endpoint.enabled}
          onChange={(value) => patch({ enabled: value })}
        />
        <SwitchRow
          title={strings.endpointWifiOnly}
          subtitle={strings.endpointWifiOnlyDesc}
          checked={endpoint.wifiOnly}
          onChange={(value) => patch({ wifiOnly: value })}
        />

        <code className="form-preview accent">{endpointChainPreview(endpoint)}</code>
        <SecureNotice text={strings.editorSecretNotice} />
        {error ? <WarningBox text={error} danger /> : null}
      </div>
      <div className="dialog-actions">
        <button type="button" className="btn ghost" onClick={onClose}>
          {strings.dialogCancel}
        </button>
        <button type="button" className="btn primary" onClick={checkAndSave}>
          <Icon name="check" size={16} />
          {strings.endpointSave}
        </button>
      </div>

      {jumpEditor ? (
        <JumpDialog
          initial={jumpEditor.jump}
          onClose={() => setJumpEditor(null)}
          onSave={(jump) => {
            const next = [...endpoint.jumps];
            if (jumpEditor.index >= 0) next[jumpEditor.index] = jump;
            else next.push(jump);
            patch({ jumps: next });
            setJumpEditor(null);
          }}
        />
      ) : null}
    </Modal>
  );
}

function JumpDialog({
  initial,
  onClose,
  onSave,
}: {
  initial: JumpForm;
  onClose: () => void;
  onSave: (jump: JumpForm) => void;
}) {
  const [jump, setJump] = useState<JumpForm>(initial);
  const [error, setError] = useState<string | null>(null);
  const isNew = initial.host === "" && initial.id === null;

  function patch(update: Partial<JumpForm>) {
    setJump((current) => ({ ...current, ...update }));
  }

  return (
    <Modal
      title={isNew ? strings.jumpAddTitle : strings.jumpEditTitle}
      onClose={onClose}
    >
      <div className="modal-body">
        <Field
          label={strings.jumpLabel}
          placeholder={strings.jumpLabelHint}
          value={jump.label}
          onChange={(value) => patch({ label: value })}
        />
        <div className="field-row">
          <Field
            label={strings.jumpHost}
            placeholder={strings.editorSshHostHint}
            value={jump.host}
            onChange={(value) => patch({ host: value })}
          />
          <Field
            label={strings.jumpPort}
            value={jump.port}
            inputMode="numeric"
            onChange={(value) => patch({ port: value.replace(/\D/g, "").slice(0, 5) })}
          />
        </div>
        <Field
          label={strings.jumpUsername}
          value={jump.username}
          onChange={(value) => patch({ username: value })}
        />
        <Segmented
          value={jump.authType}
          onChange={(value) => patch({ authType: value })}
          options={[
            { value: "password", label: strings.editorAuthPassword, icon: "key" },
            { value: "private_key", label: strings.editorAuthKey, icon: "lock" },
          ]}
        />
        {jump.authType === "password" ? (
          <Field
            label={
              hasSavedPassword(jump) ? strings.jumpPasswordSaved : strings.jumpPassword
            }
            placeholder={
              hasSavedPassword(jump) ? strings.editorSecretKeepHint : undefined
            }
            value={jump.passwordInput}
            type="password"
            onChange={(value) => patch({ passwordInput: value, passwordCleared: false })}
          />
        ) : (
          <>
            <LocalKeyPicker
              value={jump.privateKeyPath}
              hasSavedKey={hasSavedPrivateKey(jump)}
              onChange={(name) => patch({ privateKeyPath: name })}
            />
            {usesLocalKey(jump) ? null : (
              <Field
                label={
                  hasSavedPrivateKey(jump)
                    ? strings.jumpPrivateKeySaved
                    : strings.jumpPrivateKey
                }
                placeholder={
                  hasSavedPrivateKey(jump)
                    ? strings.editorSecretKeepHint
                    : strings.editorPrivateKeyHint
                }
                value={jump.privateKeyInput}
                multiline
                onChange={(value) =>
                  patch({ privateKeyInput: value, privateKeyCleared: false })
                }
              />
            )}
            <Field
              label={strings.jumpPassphrase}
              value={jump.passphraseInput}
              type="password"
              onChange={(value) =>
                patch({ passphraseInput: value, passphraseCleared: false })
              }
            />
          </>
        )}
        <p className="field-hint">{strings.jumpHelp}</p>
        {usesInheritedCredentials(jump) ? (
          <WarningBox
            text={strings.jumpInherited(jump.inheritedFrom ?? "")}
          />
        ) : null}
        {error ? <WarningBox text={error} danger /> : null}
      </div>
      <div className="dialog-actions">
        <button type="button" className="btn ghost" onClick={onClose}>
          {strings.dialogCancel}
        </button>
        <button
          type="button"
          className="btn primary"
          onClick={() => {
            const validation = jumpError(jump);
            if (validation) setError(validation);
            else onSave(jump);
          }}
        >
          {strings.jumpSave}
        </button>
      </div>
    </Modal>
  );
}
