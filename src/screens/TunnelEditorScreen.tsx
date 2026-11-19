import { useRef, useState, type ReactNode } from "react";
import type { AppView, AuthType, TunnelInput, TunnelView } from "../api";
import { Icon, Modal, SecureNotice, WarningBox } from "../components/ui";
import {
  MAX_JUMPS,
  chainPreview,
  hasSavedPassphrase,
  hasSavedPassword,
  hasSavedPrivateKey,
  jumpError,
  jumpFormFrom,
  signature,
  toTunnelInput,
  tunnelFormFrom,
  validateTunnel,
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
  const [jumpEditor, setJumpEditor] = useState<{
    index: number;
    jump: JumpForm;
  } | null>(null);

  const editing = initial !== null;
  const dirty = signature(form) !== baseline.current;
  const usedCount = editing ? usedBy.length : 0;

  function patch(update: Partial<TunnelForm>) {
    setForm((current) => ({ ...current, ...update }));
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

        <FormCard title={strings.editorSshSection}>
          <div className="field-row">
            <Field
              label={strings.editorSshHost}
              placeholder={strings.editorSshHostHint}
              value={form.sshHost}
              error={errors.sshHost}
              onChange={(value) => {
                patch({ sshHost: value });
                setErrors(({ sshHost: _host, ...rest }) => rest);
              }}
            />
            <Field
              label={strings.editorSshPort}
              value={form.sshPort}
              inputMode="numeric"
              error={errors.sshPort}
              onChange={(value) => {
                patch({ sshPort: value.replace(/\D/g, "").slice(0, 5) });
                setErrors(({ sshPort: _port, ...rest }) => rest);
              }}
            />
          </div>
          <Field
            label={strings.editorUsername}
            value={form.sshUser}
            error={errors.sshUser}
            onChange={(value) => {
              patch({ sshUser: value });
              setErrors(({ sshUser: _user, ...rest }) => rest);
            }}
          />

          <Segmented
            value={form.authType}
            onChange={(value) => patch({ authType: value })}
            options={[
              { value: "password", label: strings.editorAuthPassword, icon: "key" },
              { value: "private_key", label: strings.editorAuthKey, icon: "lock" },
            ]}
          />

          {form.authType === "password" ? (
            <div className="secret-block">
              <Field
                label={
                  hasSavedPassword(form)
                    ? strings.editorPasswordSaved
                    : strings.editorPassword
                }
                placeholder={
                  hasSavedPassword(form) ? strings.editorSecretKeepHint : undefined
                }
                value={form.passwordInput}
                type="password"
                error={errors.password}
                onChange={(value) => {
                  patch({ passwordInput: value, passwordCleared: false });
                  setErrors(({ password: _password, ...rest }) => rest);
                }}
              />
              {hasSavedPassword(form) ? (
                <button
                  type="button"
                  className="link-danger"
                  onClick={() => patch({ passwordInput: "", passwordCleared: true })}
                >
                  {strings.editorClearSecret}
                </button>
              ) : null}
            </div>
          ) : (
            <div className="secret-block">
              <Field
                label={
                  hasSavedPrivateKey(form)
                    ? strings.editorPrivateKeySaved
                    : strings.editorPrivateKey
                }
                placeholder={
                  hasSavedPrivateKey(form)
                    ? strings.editorSecretKeepHint
                    : strings.editorPrivateKeyHint
                }
                value={form.privateKeyInput}
                multiline
                error={errors.privateKey}
                onChange={(value) => {
                  patch({ privateKeyInput: value, privateKeyCleared: false });
                  setErrors(({ privateKey: _key, ...rest }) => rest);
                }}
              />
              <Field
                label={
                  hasSavedPassphrase(form)
                    ? `${strings.editorPassphrase} · ${strings.editorPasswordSaved}`
                    : strings.editorPassphrase
                }
                placeholder={
                  hasSavedPassphrase(form) ? strings.editorSecretKeepHint : undefined
                }
                value={form.passphraseInput}
                type="password"
                onChange={(value) =>
                  patch({ passphraseInput: value, passphraseCleared: false })
                }
              />
              {hasSavedPrivateKey(form) ? (
                <button
                  type="button"
                  className="link-danger"
                  onClick={() =>
                    patch({
                      privateKeyInput: "",
                      privateKeyCleared: true,
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
          <SecureNotice text={strings.editorSecretNotice} />
        </FormCard>

        <FormCard
          title={strings.editorJumpSection}
          subtitle={strings.editorJumpHelp}
        >
          {form.jumps.length === 0 ? (
            <p className="muted-row">
              <Icon name="shield" size={16} />
              {strings.editorDirect}
            </p>
          ) : (
            <div className="jump-list">
              {form.jumps.map((jump, index) => (
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
                  </button>
                  <div className="jump-tools">
                    <button
                      type="button"
                      className="icon-btn sm"
                      disabled={index === 0}
                      title={strings.editorJumpMoveUp}
                      aria-label={strings.editorJumpMoveUp}
                      onClick={() => {
                        const jumps = [...form.jumps];
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
                      disabled={index === form.jumps.length - 1}
                      title={strings.editorJumpMoveDown}
                      aria-label={strings.editorJumpMoveDown}
                      onClick={() => {
                        const jumps = [...form.jumps];
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
                          jumps: form.jumps.filter((_, i) => i !== index),
                        })
                      }
                    >
                      <Icon name="trash" size={15} />
                    </button>
                  </div>
                </div>
              ))}
              <p className="chain-line mono">
                {strings.editorChainPreview}: {chainPreview(form)}
              </p>
            </div>
          )}
          {errors.jumps ? <WarningBox text={errors.jumps} danger /> : null}
          <button
            type="button"
            className="btn outline sm"
            disabled={form.jumps.length >= MAX_JUMPS}
            onClick={() => setJumpEditor({ index: -1, jump: jumpFormFrom() })}
          >
            <Icon name="plus" size={16} />
            {strings.editorAddJump}
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

      {jumpEditor ? (
        <JumpDialog
          initial={jumpEditor.jump}
          onClose={() => setJumpEditor(null)}
          onSave={(jump) => {
            const next = [...form.jumps];
            if (jumpEditor.index >= 0) next[jumpEditor.index] = jump;
            else next.push(jump);
            patch({ jumps: next });
            setErrors(({ jumps: _jumps, ...rest }) => rest);
            setJumpEditor(null);
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

function Segmented({
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
