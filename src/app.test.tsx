import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { App } from "./app";
import type { CompanionSnapshot, ConnectionView, RuntimeView } from "./model";

const mocks = vi.hoisted(() => ({
  getAppState: vi.fn(() => new Promise<RuntimeView>(() => {})),
  subscribeToRuntime: vi.fn(),
  retryConnection: vi.fn(() => Promise.resolve()),
  onConnection: undefined as ((connection: ConnectionView) => void) | undefined,
  onSnapshot: undefined as ((snapshot: CompanionSnapshot) => void) | undefined,
}));

vi.mock("./adapter", () => ({
  closeWindow: vi.fn(() => Promise.resolve()),
  getAppState: mocks.getAppState,
  minimizeWindow: vi.fn(() => Promise.resolve()),
  retryConnection: mocks.retryConnection,
  setAlwaysOnTop: vi.fn(() => Promise.resolve()),
  subscribeToRuntime: mocks.subscribeToRuntime,
}));

const connection = (status: ConnectionView["status"]): ConnectionView => ({
  status,
  detail: `detail-${status}`,
  stale: status === "disconnected",
  serverVersion: "0.8.2-preview",
});

afterEach(() => {
  cleanup();
  mocks.onConnection = undefined;
  mocks.onSnapshot = undefined;
  vi.clearAllMocks();
});

function renderApp() {
  mocks.subscribeToRuntime.mockImplementationOnce(async (
    onSnapshot: (snapshot: CompanionSnapshot) => void,
    onConnection: (view: ConnectionView) => void,
  ) => {
    mocks.onConnection = onConnection;
    mocks.onSnapshot = onSnapshot;
    return vi.fn();
  });
  return render(<App />);
}

async function setConnection(status: ConnectionView["status"]) {
  renderApp();
  await waitFor(() => expect(mocks.onConnection).toBeDefined());
  mocks.onConnection?.(connection(status));
  await waitFor(() => expect(screen.getByText(
    status === "connected" ? "已连接" : status === "connecting" ? "连接中" : status === "disconnected" ? "已断开" : "版本不兼容",
  )).toBeTruthy());
}

it("shows connected status in the compact footer without a top connection panel", async () => {
  await setConnection("connected");

  expect(screen.queryByText("Herdr 已连接")).toBeNull();
  expect(screen.getByText("Protocol –")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "重试" })).toBeNull();
  expect(screen.getByText("已连接").parentElement?.getAttribute("title"))
    .toBe("detail-connected · 0.8.2-preview");
});

it("shows a disabled retry button while connecting", async () => {
  await setConnection("connecting");

  expect(screen.getByText("连接中")).toBeTruthy();
  expect(screen.getByRole("button", { name: "重试" })).toHaveProperty("disabled", true);
});

it("shows an enabled retry button after disconnection and invokes retry", async () => {
  await setConnection("disconnected");

  const retry = screen.getByRole("button", { name: "重试" });
  expect(retry).toHaveProperty("disabled", false);
  fireEvent.click(retry);
  expect(mocks.retryConnection).toHaveBeenCalledOnce();
});

it("preserves newer snapshot and connection events while the initial read is pending", async () => {
  let resolve!: (view: RuntimeView) => void;
  mocks.getAppState.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  renderApp();
  await waitFor(() => expect(mocks.getAppState).toHaveBeenCalledOnce());
  const snapshot: CompanionSnapshot = {
    version: "test", protocol: 22, capturedAt: 2000, focusedWorkspaceId: null,
    agents: [], workspaces: [{ id: "new", number: 1, label: "Latest workspace", focused: false }],
  };
  await act(async () => {
    mocks.onSnapshot?.(snapshot);
    mocks.onConnection?.(connection("connected"));
    resolve({ snapshot: null, connection: connection("connecting") });
  });
  expect(screen.getByRole("button", { name: /Latest workspace/ })).toBeTruthy();
  expect(screen.getByText("已连接")).toBeTruthy();
  expect(screen.getByText("Protocol 22")).toBeTruthy();
});

it("keeps initial snapshot data when only the connection changes during startup", async () => {
  let resolve!: (view: RuntimeView) => void;
  mocks.getAppState.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  renderApp();
  await waitFor(() => expect(mocks.getAppState).toHaveBeenCalledOnce());
  await act(async () => {
    mocks.onConnection?.(connection("disconnected"));
    resolve({
      snapshot: { version: "test", protocol: 22, capturedAt: 0, focusedWorkspaceId: null, agents: [], workspaces: [] },
      connection: connection("connected"),
    });
  });
  expect(screen.getByText("Protocol 22")).toBeTruthy();
  expect(screen.getByText("已断开")).toBeTruthy();
  expect(screen.getByText("Snapshot 已过期")).toBeTruthy();
});

it("releases listeners that finish registering after unmount", async () => {
  let resolve!: (stop: () => void) => void;
  const stop = vi.fn();
  mocks.subscribeToRuntime.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  const app = render(<App />);
  await waitFor(() => expect(mocks.subscribeToRuntime).toHaveBeenCalledOnce());
  app.unmount();
  await act(async () => { resolve(stop); });
  expect(stop).toHaveBeenCalledOnce();
  expect(mocks.getAppState).not.toHaveBeenCalled();
});

it("keeps the custom titlebar outside the scrolling region", () => {
  const app = renderApp();
  const titlebar = document.querySelector<HTMLElement>(".titlebar");
  expect(titlebar?.parentElement?.className).toBe("app-shell");

  const styles = readFileSync("src/styles.css", "utf8");
  expect(styles).toMatch(/body\s*{[^}]*overflow:\s*hidden/);
  expect(styles).toMatch(/\.app-content\s*{[^}]*overflow:\s*hidden/);
  expect(styles).toMatch(/\.agent-list\s*{[^}]*overflow-y:\s*auto/);
  expect(styles).toMatch(/\.app-content\s*{[^}]*padding:\s*var\(--main-padding\)\s+8px\s+8px/);
  expect(styles).toMatch(/\.agent-card\s*{[^}]*padding:\s*var\(--card-padding-block\)\s+8px/);
  expect(styles).not.toContain("@media (max-width: 390px)");
  app.unmount();
});

it("marks only non-interactive titlebar content as a drag region", () => {
  const app = renderApp();
  const titlebar = document.querySelector<HTMLElement>(".titlebar");
  const brand = document.querySelector<HTMLElement>(".titlebar-brand");
  const mark = document.querySelector<HTMLElement>(".titlebar-mark");
  const title = screen.getByText("Herdr Companion", { selector: "span" });
  const controls = document.querySelector<HTMLElement>(".window-controls");

  expect(titlebar?.hasAttribute("data-tauri-drag-region")).toBe(true);
  expect(brand?.hasAttribute("data-tauri-drag-region")).toBe(true);
  expect(mark?.hasAttribute("data-tauri-drag-region")).toBe(true);
  expect(title.hasAttribute("data-tauri-drag-region")).toBe(true);
  expect(controls?.hasAttribute("data-tauri-drag-region")).toBe(false);
  for (const button of controls?.querySelectorAll("button") ?? []) {
    expect(button.hasAttribute("data-tauri-drag-region")).toBe(false);
  }
  app.unmount();
});
