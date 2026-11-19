import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  errorMessage,
  onAppStatus,
  type AppInput,
  type AppStatus,
  type DockView,
  type GroupView,
  type TunnelInput,
} from "./api";
import { BottomNav, Icon, Modal, type NavTab } from "./components/ui";
import { AppsScreen } from "./screens/AppsScreen";
import { AppEditorScreen } from "./screens/AppEditorScreen";
import { ErrorDetailScreen } from "./screens/ErrorDetailScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TunnelsScreen, type TestEntry } from "./screens/TunnelsScreen";
import { TunnelEditorScreen } from "./screens/TunnelEditorScreen";
import { nextPort } from "./draft";
import { resolveAppStatus } from "./status";
import { strings, type ThemeMode } from "./strings";

type OverlaySpec =
  | {
      kind: "app-editor";
      appId: string | null;
      defaults: { groupId: string; tunnelId: string; localPort: number };
    }
  | { kind: "tunnel-editor"; tunnelId: string | null }
  | { kind: "error-detail"; appId: string };

type Overlay = OverlaySpec & { key: string };

type StatusMap = Record<string, AppStatus>;
type TestMap = Record<string, TestEntry>;

const THEME_KEY = "tunneldock-theme";
const EMPTY_DOCK: DockView = { tunnels: [], apps: [], groups: [], statuses: [] };

function initialTheme(): ThemeMode {
  const stored = localStorage.getItem(THEME_KEY);
  return stored === "dark" || stored === "light" || stored === "system"
    ? stored
    : "system";
}

export default function App() {
  const [dock, setDock] = useState<DockView>(EMPTY_DOCK);
  const [statuses, setStatuses] = useState<StatusMap>({});
  const [tab, setTab] = useState<NavTab>("apps");
  const [overlays, setOverlays] = useState<Overlay[]>([]);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const [tests, setTests] = useState<TestMap>({});
  const [theme, setTheme] = useState<ThemeMode>(initialTheme);
  const [knownHostsCount, setKnownHostsCount] = useState(0);
  const [version, setVersion] = useState("1.0.0");
  const [groupDialog, setGroupDialog] = useState<GroupView | "new" | null>(null);
  const [toast, setToast] = useState<{ key: number; text: string } | null>(null);
  const toastTimer = useRef<number | null>(null);
  const overlaySeq = useRef(0);

  const notify = useCallback((text: string) => {
    if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    setToast({ key: Date.now(), text });
    toastTimer.current = window.setTimeout(() => setToast(null), 2600);
  }, []);

  const mergeStatuses = useCallback(
    (current: StatusMap, incoming: AppStatus[], appIds: Set<string>) => {
      const merged: StatusMap = {};
      for (const [id, status] of Object.entries(current)) {
        if (appIds.has(id)) merged[id] = status;
      }
      for (const status of incoming) {
        if (!appIds.has(status.appId)) continue;
        const existing = merged[status.appId];
        if (!existing || status.updatedAt >= existing.updatedAt) {
          merged[status.appId] = status;
        }
      }
      return merged;
    },
    [],
  );

  const refreshDock = useCallback(async () => {
    const next = await api.loadDock();
    setDock(next);
    const appIds = new Set(next.apps.map((app) => app.id));
    setStatuses((current) => mergeStatuses(current, next.statuses, appIds));
    setTests((current) => {
      const tunnelIds = new Set(next.tunnels.map((tunnel) => tunnel.id));
      const kept: TestMap = {};
      for (const [id, value] of Object.entries(current)) {
        if (tunnelIds.has(id)) kept[id] = value;
      }
      return kept;
    });
    return next;
  }, [mergeStatuses]);

  const refreshKnownHosts = useCallback(async () => {
    try {
      setKnownHostsCount(await api.knownHostsCount());
    } catch {
      // A count failure must not break the UI.
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    const pending = new Map<string, AppStatus>();

    void (async () => {
      try {
        unlisten = await onAppStatus((status) => {
          pending.set(status.appId, status);
          setStatuses((current) => {
            const existing = current[status.appId];
            if (existing && existing.updatedAt > status.updatedAt) return current;
            return { ...current, [status.appId]: status };
          });
        });
        if (disposed) {
          unlisten();
          return;
        }

        const [next, appVersion] = await Promise.all([
          api.loadDock(),
          api.appVersion(),
        ]);
        if (disposed) return;
        setDock(next);
        setVersion(appVersion);
        const appIds = new Set(next.apps.map((app) => app.id));
        setStatuses(() => {
          const merged: StatusMap = {};
          for (const status of next.statuses) {
            merged[status.appId] = status;
          }
          for (const [id, status] of pending) {
            if (appIds.has(id)) merged[id] = status;
          }
          return merged;
        });
        void refreshKnownHosts();
      } catch (error) {
        if (!disposed) notify(errorMessage(error));
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
      if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    };
  }, [notify, refreshKnownHosts]);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  const push = useCallback((overlay: OverlaySpec) => {
    overlaySeq.current += 1;
    setOverlays((current) => [
      ...current,
      { ...overlay, key: `screen-${overlaySeq.current}` } as Overlay,
    ]);
  }, []);

  const pop = useCallback(() => {
    setOverlays((current) => current.slice(0, -1));
  }, []);

  function selectTab(next: NavTab) {
    setTab(next);
    setOverlays([]);
    if (next === "settings") void refreshKnownHosts();
  }

  function toggleGroup(id: string) {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function openCreateApp(groupId?: string) {
    push({
      kind: "app-editor",
      appId: null,
      defaults: {
        groupId: groupId ?? dock.groups[0]?.id ?? "default",
        tunnelId: dock.tunnels[0]?.id ?? "",
        localPort: nextPort(dock.apps),
      },
    });
  }

  function openEditApp(id: string) {
    push({
      kind: "app-editor",
      appId: id,
      defaults: { groupId: "default", tunnelId: "", localPort: 18080 },
    });
  }

  function openCreateTunnel() {
    push({ kind: "tunnel-editor", tunnelId: null });
  }

  function openEditTunnel(id: string) {
    push({ kind: "tunnel-editor", tunnelId: id });
  }

  function openError(id: string) {
    push({ kind: "error-detail", appId: id });
  }

  /** Runs an action, reporting failures as a toast. Never throws. */
  async function run(action: () => Promise<unknown>): Promise<boolean> {
    try {
      await action();
      return true;
    } catch (error) {
      notify(errorMessage(error));
      return false;
    }
  }

  async function saveApp(input: AppInput) {
    await api.saveApp(input);
    await refreshDock();
    setOverlays((current) =>
      current[current.length - 1]?.kind === "app-editor"
        ? current.slice(0, -1)
        : current,
    );
    notify(strings.appEditorSaved);
  }

  async function saveTunnel(input: TunnelInput) {
    const affected = dock.apps.filter((app) => app.tunnelId === input.id).length;
    const saved = await api.saveTunnel(input);
    await refreshDock();
    setOverlays((current) => {
      const next = current.slice(0, -1);
      const top = next[next.length - 1];
      // Returning from a tunnel created inside the application editor: pin the
      // new tunnel so the half-filled form does not need a manual reselect.
      if (top?.kind === "app-editor" && top.appId === null) {
        next[next.length - 1] = {
          ...top,
          defaults: { ...top.defaults, tunnelId: saved.id },
        };
      }
      return next;
    });
    notify(
      affected > 0
        ? strings.editorTunnelSavedStopped(affected)
        : strings.editorTunnelSaved,
    );
  }

  async function deleteApp(id: string) {
    await api.deleteApp(id);
    await refreshDock();
    setOverlays((current) => {
      const top = current[current.length - 1];
      if (top?.kind === "app-editor" && top.appId === id) return current.slice(0, -1);
      if (top?.kind === "error-detail" && top.appId === id) return current.slice(0, -1);
      return current;
    });
    notify(strings.toastDeleted);
  }

  async function deleteTunnel(id: string) {
    const affected = dock.apps.filter((app) => app.tunnelId === id).length;
    await api.deleteTunnel(id);
    await refreshDock();
    setOverlays((current) => {
      const top = current[current.length - 1];
      return top?.kind === "tunnel-editor" && top.tunnelId === id
        ? current.slice(0, -1)
        : current;
    });
    notify(strings.toastTunnelDeleted(affected));
  }

  async function saveGroup(id: string | null, name: string) {
    await api.saveGroup({ id, name });
    await refreshDock();
    setGroupDialog(null);
  }

  async function deleteGroup(group: GroupView) {
    const deleted = await run(() => api.deleteGroup(group.id));
    await refreshDock();
    if (deleted) notify(strings.toastGroupDeleted);
  }

  async function testTunnel(id: string) {
    setTests((current) => ({ ...current, [id]: "testing" }));
    try {
      const result = await api.testTunnel(id);
      setTests((current) => ({ ...current, [id]: result }));
    } catch (error) {
      setTests((current) => ({
        ...current,
        [id]: {
          tunnelId: id,
          state: "failure",
          elapsedMs: 0,
          errorCode: "generic",
          errorDetail: errorMessage(error),
          securityAlert: false,
        },
      }));
    }
  }

  function openBrowser(url: string) {
    void api.openUrl(url).catch(() => notify(strings.toastNoBrowser));
  }

  function copy(text: string) {
    void navigator.clipboard
      .writeText(text)
      .catch(() => undefined)
      .then(() => notify(strings.appCopied));
  }

  const appMap = new Map(dock.apps.map((app) => [app.id, app]));
  const tunnelMap = new Map(dock.tunnels.map((tunnel) => [tunnel.id, tunnel]));
  const groupMap = new Map(dock.groups.map((group) => [group.id, group]));
  const autostart = dock.apps.some((app) => app.autoStartOnBoot);

  function renderOverlay(overlay: Overlay) {
    if (overlay.kind === "app-editor") {
      return (
        <AppEditorScreen
          apps={dock.apps}
          tunnels={dock.tunnels}
          groups={dock.groups}
          initial={overlay.appId ? appMap.get(overlay.appId) ?? null : null}
          defaults={overlay.defaults}
          onClose={pop}
          onSave={saveApp}
          onDelete={deleteApp}
          onCreateTunnel={openCreateTunnel}
        />
      );
    }
    if (overlay.kind === "tunnel-editor") {
      const initial = overlay.tunnelId
        ? tunnelMap.get(overlay.tunnelId) ?? null
        : null;
      return (
        <TunnelEditorScreen
          initial={initial}
          usedBy={
            initial
              ? dock.apps.filter((app) => app.tunnelId === initial.id)
              : []
          }
          onClose={pop}
          onSave={saveTunnel}
          onDelete={deleteTunnel}
        />
      );
    }
    const app = appMap.get(overlay.appId);
    const status =
      app !== undefined
        ? resolveAppStatus(app, dock.tunnels, dock.groups, statuses)
        : undefined;
    return (
      <ErrorDetailScreen
        app={app}
        tunnel={app ? tunnelMap.get(app.tunnelId) : undefined}
        group={app ? groupMap.get(app.groupId) : undefined}
        status={status}
        onBack={pop}
        onEdit={() => {
          setOverlays((current) => current.slice(0, -1));
          openEditApp(overlay.appId);
        }}
        onRetry={() => void run(() => api.restartApp(overlay.appId))}
      />
    );
  }

  return (
    <main className="app-root">
      <div className={overlays.length > 0 ? "tab-pages is-hidden" : "tab-pages"}>
        {tab === "apps" ? (
          <AppsScreen
            apps={dock.apps}
            tunnels={dock.tunnels}
            groups={dock.groups}
            statuses={statuses}
            collapsedGroups={collapsedGroups}
            onToggleGroup={toggleGroup}
            onStartApp={(id) => void run(() => api.startApp(id))}
            onStopApp={(id) => void run(() => api.stopApp(id))}
            onRestartApp={(id) => void run(() => api.restartApp(id))}
            onEditApp={openEditApp}
            onDeleteApp={(id) => void run(() => deleteApp(id))}
            onOpenError={openError}
            onOpenBrowser={openBrowser}
            onCopy={copy}
            onStartGroup={(id) => void run(() => api.startGroup(id))}
            onStopGroup={(id) => void run(() => api.stopGroup(id))}
            onCreateApp={openCreateApp}
            onCreateTunnel={openCreateTunnel}
            onCreateGroup={() => setGroupDialog("new")}
            onRenameGroup={(group) => setGroupDialog(group)}
            onDeleteGroup={(group) => void run(() => deleteGroup(group))}
          />
        ) : null}

        {tab === "tunnels" ? (
          <TunnelsScreen
            tunnels={dock.tunnels}
            apps={dock.apps}
            statuses={statuses}
            tests={tests}
            onTest={(id) => void testTunnel(id)}
            onEdit={openEditTunnel}
            onDelete={(id) => void run(() => deleteTunnel(id))}
            onCreate={openCreateTunnel}
          />
        ) : null}

        {tab === "settings" ? (
          <SettingsScreen
            theme={theme}
            knownHostsCount={knownHostsCount}
            autostart={autostart}
            version={version}
            tunnelCount={dock.tunnels.length}
            appCount={dock.apps.length}
            groupCount={dock.groups.length}
            onThemeChange={setTheme}
            onClearKnownHosts={async () => {
              await api.clearKnownHosts();
              await refreshKnownHosts();
            }}
            onMessage={notify}
          />
        ) : null}
      </div>

      {overlays.map((overlay, index) => (
        <div
          key={overlay.key}
          className={
            index === overlays.length - 1
              ? "overlay-screen"
              : "overlay-screen is-hidden"
          }
          aria-hidden={index !== overlays.length - 1}
        >
          {renderOverlay(overlay)}
        </div>
      ))}

      {overlays.length === 0 ? (
        <BottomNav tab={tab} onChange={selectTab} />
      ) : null}

      {groupDialog ? (
        <GroupDialog
          group={groupDialog === "new" ? null : groupDialog}
          onClose={() => setGroupDialog(null)}
          onSave={(name) =>
            saveGroup(groupDialog === "new" ? null : groupDialog.id, name)
          }
        />
      ) : null}

      {toast ? (
        <div className="toast" key={toast.key} role="status">
          {toast.text}
        </div>
      ) : null}
    </main>
  );
}

function GroupDialog({
  group,
  onClose,
  onSave,
}: {
  group: GroupView | null;
  onClose: () => void;
  onSave: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState(group?.name ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function submit() {
    if (name.trim() === "") {
      setError(strings.groupNameRequired);
      return;
    }
    setSaving(true);
    try {
      await onSave(name.trim());
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <Modal
      title={group ? strings.groupNameTitleEdit : strings.groupNameTitleNew}
      onClose={onClose}
      small
    >
      <div className="modal-body">
        <label className={`field${error ? " has-error" : ""}`}>
          <span className="field-label">{strings.groupNameLabel}</span>
          <input
            value={name}
            placeholder={strings.groupNameHint}
            autoFocus
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submit();
            }}
          />
          {error ? <span className="field-error">{error}</span> : null}
        </label>
      </div>
      <div className="dialog-actions">
        <button type="button" className="btn ghost" onClick={onClose}>
          {strings.dialogCancel}
        </button>
        <button
          type="button"
          className="btn primary"
          disabled={saving}
          onClick={() => void submit()}
        >
          {saving ? <span className="spinner light" /> : <Icon name="check" size={16} />}
          {strings.groupNameSave}
        </button>
      </div>
    </Modal>
  );
}
