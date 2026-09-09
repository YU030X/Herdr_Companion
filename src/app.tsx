import { useEffect, useState } from "preact/hooks";
import {
  closeWindow,
  getAppState,
  minimizeWindow,
  retryConnection,
  setAlwaysOnTop,
  subscribeToRuntime,
} from "./adapter";
import { Hud } from "./components/hud";
import type { RuntimeView } from "./model";

const ALWAYS_ON_TOP_KEY = "herdr-companion:always-on-top";

const INITIAL_STATE: RuntimeView = {
  connection: {
    status: "connecting",
    detail: "正在启动 Companion",
    stale: false,
    serverVersion: null,
  },
  snapshot: null,
};

export function App() {
  const [runtime, setRuntime] = useState<RuntimeView>(INITIAL_STATE);
  const [receivedAt, setReceivedAt] = useState(Date.now());
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("all");
  const [alwaysOnTop, setAlwaysOnTopState] = useState(readAlwaysOnTop);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    let snapshotEvents = 0;
    let connectionEvents = 0;

    void (async () => {
      try {
        unsubscribe = await subscribeToRuntime(
          (snapshot) => {
            if (disposed) return;
            snapshotEvents += 1;
            setReceivedAt(Date.now());
            setRuntime((current) => ({ ...current, snapshot }));
          },
          (connection) => {
            if (disposed) return;
            connectionEvents += 1;
            setRuntime((current) => ({ ...current, connection }));
          },
        );
        if (disposed) {
          unsubscribe();
          return;
        }
        const snapshotVersion = snapshotEvents;
        const connectionVersion = connectionEvents;
        const current = await getAppState();
        if (!disposed) {
          // Events received during the initial read take precedence per field.
          setRuntime((latest) => ({
            snapshot: snapshotEvents === snapshotVersion ? current.snapshot : latest.snapshot,
            connection: connectionEvents === connectionVersion ? current.connection : latest.connection,
          }));
          if (snapshotEvents === snapshotVersion) setReceivedAt(Date.now());
        }
      } catch (error) {
        if (!disposed) setMessage(errorMessage(error));
      }
    })();

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);

  useEffect(() => {
    if (
      selectedWorkspaceId !== "all"
      && !runtime.snapshot?.workspaces.some((workspace) => workspace.id === selectedWorkspaceId)
    ) {
      setSelectedWorkspaceId("all");
    }
  }, [runtime.snapshot, selectedWorkspaceId]);

  useEffect(() => {
    void setAlwaysOnTop(alwaysOnTop).catch((error) => setMessage(errorMessage(error)));
  }, []);

  async function handleRetry() {
    setMessage(null);
    try {
      await retryConnection();
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  async function handleAlwaysOnTop() {
    const next = !alwaysOnTop;
    setMessage(null);
    try {
      await setAlwaysOnTop(next);
      setAlwaysOnTopState(next);
      window.localStorage.setItem(ALWAYS_ON_TOP_KEY, String(next));
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  return (
    <main class="app-shell">
      <header class="titlebar" data-tauri-drag-region>
        <div class="titlebar-brand" data-tauri-drag-region>
          <span class="titlebar-mark" data-tauri-drag-region aria-hidden="true">H</span>
          <span data-tauri-drag-region>Herdr Companion</span>
        </div>
        <div class="window-controls">
          <button
            class={alwaysOnTop ? "window-button pin-button is-active" : "window-button pin-button"}
            type="button"
            title={alwaysOnTop ? "取消置顶" : "始终置顶"}
            aria-label={alwaysOnTop ? "取消置顶" : "始终置顶"}
            aria-pressed={alwaysOnTop}
            onClick={handleAlwaysOnTop}
          >
            <PinIcon />
          </button>
          <button
            class="window-button"
            type="button"
            title="最小化"
            aria-label="最小化"
            onClick={() => void minimizeWindow()}
          >
            <span class="minimize-icon" aria-hidden="true" />
          </button>
          <button
            class="window-button close-button"
            type="button"
            title="关闭"
            aria-label="关闭"
            onClick={() => void closeWindow()}
          >
            <span class="close-icon" aria-hidden="true">×</span>
          </button>
        </div>
      </header>

      <div class="app-content">
        {message && (
        <div class="error-banner" role="alert">
          <span>{message}</span>
          <button type="button" aria-label="关闭错误" onClick={() => setMessage(null)}>×</button>
        </div>
      )}

      <Hud
        connection={runtime.connection}
        snapshot={runtime.snapshot}
        receivedAt={receivedAt}
        selectedWorkspaceId={selectedWorkspaceId}
        onSelectWorkspace={setSelectedWorkspaceId}
      />

        <footer class="app-footer">
          <div
            class={`footer-connection status-${runtime.connection.status}`}
            title={[runtime.connection.detail, runtime.connection.serverVersion].filter(Boolean).join(" · ")}
            aria-label={`连接状态：${connectionStatusLabel(runtime.connection.status)}。${runtime.connection.detail}`}
          >
            <span class="connection-dot" aria-hidden="true" />
            <span>{connectionStatusLabel(runtime.connection.status)}</span>
            {runtime.connection.status !== "connected" && (
              <button
                class="retry-button"
                type="button"
                disabled={runtime.connection.status === "connecting"}
                onClick={handleRetry}
              >
                重试
              </button>
            )}
          </div>
          <span
            class="protocol-label"
            title={runtime.connection.serverVersion ?? undefined}
          >
            Protocol {runtime.snapshot?.protocol ?? "–"}
          </span>
        </footer>
      </div>
    </main>
  );
}

function PinIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16">
      <path d="M5.2 2.2h5.6l-.8 3.3 2 2v1H8.7V14L8 14.8 7.3 14V8.5H4v-1l2-2-.8-3.3Z" />
    </svg>
  );
}

function readAlwaysOnTop(): boolean {
  try {
    return window.localStorage.getItem(ALWAYS_ON_TOP_KEY) === "true";
  } catch {
    return false;
  }
}

function connectionStatusLabel(status: RuntimeView["connection"]["status"]): string {
  switch (status) {
    case "connected": return "已连接";
    case "connecting": return "连接中";
    case "disconnected": return "已断开";
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
