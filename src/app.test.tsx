import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, expect, it, vi } from "vitest";
import { App } from "./app";
import type { ConnectionView, RuntimeView } from "./model";

const mocks = vi.hoisted(() => ({
  getAppState: vi.fn(() => new Promise<never>(() => {})),
  subscribeToRuntime: vi.fn(),
  retryConnection: vi.fn(() => Promise.resolve()),
  onConnection: undefined as ((connection: ConnectionView) => void) | undefined,
}));

vi.mock("./adapter", () => ({
  closeWindow: vi.fn(() => Promise.resolve()),
  getAppState: mocks.getAppState,
  minimizeWindow: vi.fn(() => Promise.resolve()),
  retryConnection: mocks.retryConnection,
  setAlwaysOnTop: vi.fn(() => Promise.resolve()),
  subscribeToRuntime: mocks.subscribeToRuntime,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn(() => Promise.resolve()),
  })),
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
  vi.clearAllMocks();
});

function renderApp() {
  mocks.subscribeToRuntime.mockImplementationOnce(async (
    _onSnapshot: (snapshot: RuntimeView["snapshot"]) => void,
    onConnection: (view: ConnectionView) => void,
  ) => {
    mocks.onConnection = onConnection;
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
