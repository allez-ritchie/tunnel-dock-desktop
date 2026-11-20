import { useEffect, useMemo, useState } from "react";
import { api, type LocalSshKey } from "../api";
import { Icon } from "./ui";
import { strings } from "../strings";

interface Props {
  /** 当前选中的 `~/.ssh` 文件名，未选择时为 `null`。 */
  value: string | null;
  /** 是否已经有保存好的私钥（避免误清空）。 */
  hasSavedKey: boolean;
  onChange: (name: string | null) => void;
}

interface State {
  loading: boolean;
  error: string | null;
  available: boolean;
  root: string;
  keys: LocalSshKey[];
}

const EMPTY: State = { loading: true, error: null, available: false, root: "", keys: [] };

/**
 * 从本机 `~/.ssh` 选择私钥。选中后只把**文件名**交给后端，
 * 密钥内容不会进入前端状态、日志或导出的配置包。
 */
export function LocalKeyPicker({ value, hasSavedKey, onChange }: Props) {
  const [state, setState] = useState<State>(EMPTY);

  useEffect(() => {
    let disposed = false;
    void (async () => {
      try {
        const status = await api.localSshStatus();
        if (disposed) return;
        setState({
          loading: false,
          error: null,
          available: status.available,
          root: status.root,
          keys: status.keys,
        });
      } catch (error) {
        if (disposed) return;
        setState({
          loading: false,
          error: strings.localKeyLoadFailed(String(error)),
          available: false,
          root: "",
          keys: [],
        });
      }
    })();
    return () => {
      disposed = true;
    };
  }, []);

  const options = useMemo(() => state.keys, [state.keys]);
  const selected = value ?? "";

  return (
    <div className="local-key-picker">
      <label className="field">
        <span className="field-label">{strings.localKeyUse}</span>
        <select
          value={selected}
          disabled={state.loading || !state.available}
          onChange={(event) => onChange(event.target.value || null)}
        >
          <option value="">
            {hasSavedKey ? strings.editorSecretKeepHint : strings.localKeyPick}
          </option>
          {options.map((key) => (
            <option key={key.name} value={key.name}>
              {`${key.name}${key.keyType ? ` · ${key.keyType}` : ""}${
                key.encrypted ? ` · ${strings.localKeyEncrypted}` : ""
              }`}
            </option>
          ))}
        </select>
      </label>
      <p className="field-hint">
        {state.loading
          ? strings.localKeyLoading
          : state.error
            ? state.error
            : !state.available
              ? strings.localKeyNone
              : options.length === 0
                ? strings.localKeyEmpty
                : selected
                  ? strings.localKeySelected(selected)
                  : state.root}
      </p>
      {selected ? (
        <p className="local-key-note">
          <Icon name="lock" size={13} />
          {strings.localKeyApplyHint}
        </p>
      ) : null}
    </div>
  );
}
