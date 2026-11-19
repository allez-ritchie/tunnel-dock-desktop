import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  errorMessage,
  onTunnelStatus,
  type TunnelInput,
  type TunnelStatus,
  type TunnelView,
} from "./api";
import { HomeScreen, type HomeItem } from "./screens/HomeScreen";
import { EditorScreen } from "./screens/EditorScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { strings, type ThemeMode } from "./strings";

type Screen =
  | { kind: "home" }
  | { kind: "settings" }
  | { kind: "editor"; id: string | null };

type StatusMap = Record<string, TunnelStatus>;

const THEME_KEY = "tunneldock-theme";

function initialTheme(): ThemeMode {
  const stored = localStorage.getItem(THEME_KEY);
  return stored === "dark" || stored === "light" || stored === "system"
    ? stored
    : "system";
}

export default function App() {
  const [tunnels, setTunnels] = useState<TunnelView[]>([]);
  const [statuses, setStatuses] = useState<StatusMap>({});
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const [theme, setTheme] = useState<ThemeMode>(initialTheme);
  const [knownHostsCount, setKnownHostsCount] = useState(0);
  const [version, setVersion] = useState("1.0.0");
  const [toast, setToast] = useState<{ key: number; text: string } | null>(null);
  const toastTimer = useRef<number | null>(null);

  const notify = useCallback((text: string) => {
    if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    setToast({ key: Date.now(), text });
    toastTimer.current = window.setTimeout(() => setToast(null), 2600);
  }, []);

  const refreshTunnels = useCallback(async () => {
    const list = await api.listTunnels();
    setTunnels(list);
    return list;
  }, []);

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
    // Events that arrive while the initial snapshot is in flight must not be
    // lost, otherwise a tunnel connecting during startup stays "connecting".
    const pending = new Map<string, TunnelStatus>();

    void (async () => {
      try {
        // Subscribe first, then read the snapshot, so no transition can fall
        // into the gap between the two calls.
        unlisten = await onTunnelStatus((status) => {
          pending.set(status.tunnelId, status);
          setStatuses((current) => ({ ...current, [status.tunnelId]: status }));
        });
        if (disposed) {
          unlisten();
          return;
        }

        const [list, currentStatuses, appVersion] = await Promise.all([
          api.listTunnels(),
          api.listStatuses(),
          api.appVersion(),
        ]);
        if (disposed) return;
        setTunnels(list);
        setVersion(appVersion);
        setStatuses(() => {
          const merged: StatusMap = Object.fromEntries(
            currentStatuses.map((status) => [status.tunnelId, status]),
          );
          for (const [id, status] of pending) {
            const snapshot = merged[id];
            if (!snapshot || status.updatedAt >= snapshot.updatedAt) {
              merged[id] = status;
            }
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

  const items: HomeItem[] = tunnels.map((config) => ({
    config,
    status:
      statuses[config.id] ??
      ({
        tunnelId: config.id,
        phase: config.desiredOn ? "connecting" : "idle",
        attempt: 0,
        nextRetrySeconds: null,
        connectedAt: null,
        localPort: null,
        errorCode: null,
        errorDetail: null,
        securityAlert: false,
        updatedAt: Date.now(),
      } satisfies TunnelStatus),
  }));

  const activeTunnel =
    screen.kind === "editor" && screen.id !== null
      ? tunnels.find((tunnel) => tunnel.id === screen.id) ?? null
      : null;

  function openSettings() {
    setScreen({ kind: "settings" });
    void refreshKnownHosts();
  }

  async function connect(id: string) {
    try {
      await api.connectTunnel(id);
    } catch (error) {
      notify(errorMessage(error));
    }
  }

  async function connectAll() {
    try {
      await api.connectAll();
    } catch (error) {
      notify(errorMessage(error));
    }
  }

  async function disconnect(id: string) {
    try {
      await api.disconnectTunnel(id);
    } catch (error) {
      notify(errorMessage(error));
    }
  }

  async function disconnectAll() {
    try {
      await api.disconnectAll();
    } catch (error) {
      notify(errorMessage(error));
    }
  }

  async function retry(id: string) {
    try {
      await api.reconnectTunnel(id);
    } catch (error) {
      notify(errorMessage(error));
    }
  }

  async function saveTunnel(input: TunnelInput) {
    const previous = tunnels.find((tunnel) => tunnel.id === input.id);
    await api.saveTunnel(input);
    await refreshTunnels();
    setScreen({ kind: "home" });
    notify(previous?.desiredOn ? strings.editorRestarting : strings.editorSaved);
  }

  async function deleteTunnel(id: string) {
    await api.deleteTunnel(id);
    await refreshTunnels();
    setScreen({ kind: "home" });
    notify(strings.toastDeleted);
  }

  function openBrowser(url: string) {
    void api.openUrl(url).catch(() => notify(strings.toastNoBrowser));
  }

  const autostart = tunnels.some((tunnel) => tunnel.autoStartOnBoot);

  return (
    <main className="app-root">
      {screen.kind === "home" ? (
        <HomeScreen
          items={items}
          onOpenSettings={openSettings}
          onCreate={() => setScreen({ kind: "editor", id: null })}
          onEdit={(id) => setScreen({ kind: "editor", id })}
          onConnect={(id) => void connect(id)}
          onConnectAll={() => void connectAll()}
          onDisconnect={(id) => void disconnect(id)}
          onDisconnectAll={() => void disconnectAll()}
          onRetry={(id) => void retry(id)}
          onDelete={(id) => void deleteTunnel(id)}
          onOpenBrowser={openBrowser}
          onMessage={notify}
        />
      ) : null}

      {screen.kind === "editor" ? (
        <EditorScreen
          key={screen.id ?? "new"}
          tunnels={tunnels}
          initial={activeTunnel}
          onClose={() => setScreen({ kind: "home" })}
          onSave={saveTunnel}
          onDelete={deleteTunnel}
        />
      ) : null}

      {screen.kind === "settings" ? (
        <SettingsScreen
          theme={theme}
          knownHostsCount={knownHostsCount}
          autostart={autostart}
          version={version}
          onThemeChange={setTheme}
          onBack={() => setScreen({ kind: "home" })}
          onClearKnownHosts={async () => {
            await api.clearKnownHosts();
            await refreshKnownHosts();
          }}
          onMessage={notify}
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
