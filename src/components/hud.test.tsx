import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, expect, it } from "vitest";
import { Hud } from "./hud";
import type { CompanionSnapshot, ConnectionView } from "../model";

const snapshot: CompanionSnapshot = {
  version: "0.8.2-preview",
  protocol: 21,
  capturedAt: 10_000,
  focusedWorkspaceId: "w1",
  workspaces: [{ id: "w1", number: 1, label: "Companion", focused: true }],
  agents: [
    {
      id: "pane-working",
      terminalId: "terminal-working",
      name: "Worker",
      agentType: "Codex",
      status: "working",
      workspaceId: "w1",
      tabId: "w1:t1",
      paneId: "pane-working",
      task: "Implement adapter",
      taskSource: "token",
      parentId: null,
      focused: false,
      focusTarget: "worker",
      revision: 1,
      stateChangeSeq: 1,
      firstObservedAt: 5_000,
      statusObservedAt: 5_000,
      lastEventObservedAt: 5_000,
    },
    {
      id: "pane-blocked",
      terminalId: "terminal-blocked",
      name: "Reviewer",
      agentType: "Claude",
      status: "blocked",
      workspaceId: "w1",
      tabId: "w1:t1",
      paneId: "pane-blocked",
      task: null,
      taskSource: null,
      parentId: null,
      focused: false,
      focusTarget: "reviewer",
      revision: 2,
      stateChangeSeq: 2,
      firstObservedAt: 5_000,
      statusObservedAt: 8_000,
      lastEventObservedAt: 8_000,
    },
  ],
};

const staleConnection: ConnectionView = {
  status: "disconnected",
  detail: "Herdr connection failed",
  stale: true,
  serverVersion: "0.8.2-preview",
};

const workspaceSnapshot: CompanionSnapshot = {
  ...snapshot,
  workspaces: [
    ...snapshot.workspaces,
    { id: "w2", number: 2, label: "Other", focused: false },
  ],
  agents: [
    ...snapshot.agents,
    {
      ...snapshot.agents[0],
      id: "pane-other",
      name: "Other Worker",
      workspaceId: "w2",
      tabId: "w2:t1",
      paneId: "pane-other",
      task: "Review workspace",
    },
  ],
};

afterEach(() => cleanup());

it("shows attention-first agents as non-interactive status entries", () => {
  const view = render(
    <Hud
      connection={staleConnection}
      snapshot={snapshot}
      receivedAt={Date.now()}
      selectedWorkspaceId="all"
      onSelectWorkspace={() => undefined}
    />,
  );

  expect(screen.getByRole("status").textContent).toContain("Snapshot 已过期");
  expect(screen.queryByRole("button", { name: /聚焦/ })).toBeNull();
  expect(screen.getAllByRole("article")).toHaveLength(2);
  expect(screen.getByText("暂无可信任务描述")).toBeTruthy();

  view.unmount();
});

it("filters agents when a Workspace tab is selected", () => {
  let requestedWorkspace = "all";
  const view = render(
    <Hud
      connection={{ ...staleConnection, status: "connected", stale: false }}
      snapshot={workspaceSnapshot}
      receivedAt={Date.now()}
      selectedWorkspaceId={requestedWorkspace}
      onSelectWorkspace={(workspaceId) => { requestedWorkspace = workspaceId; }}
    />,
  );

  const workspaceTab = screen.getByRole("button", { name: /Other/ });
  workspaceTab.click();
  expect(requestedWorkspace).toBe("w2");

  view.rerender(
    <Hud
      connection={{ ...staleConnection, status: "connected", stale: false }}
      snapshot={workspaceSnapshot}
      receivedAt={Date.now()}
      selectedWorkspaceId={requestedWorkspace}
      onSelectWorkspace={(workspaceId) => { requestedWorkspace = workspaceId; }}
    />,
  );

  expect(screen.getByRole("heading", { name: "Other" })).toBeTruthy();
  expect(screen.getByText("Other Worker")).toBeTruthy();
  expect(screen.queryByText("Implement adapter")).toBeNull();
  expect(screen.getAllByRole("article")).toHaveLength(1);

  view.unmount();
});

it("maps vertical wheel input to horizontal Workspace scrolling", () => {
  const view = render(
    <Hud
      connection={{ ...staleConnection, status: "connected", stale: false }}
      snapshot={workspaceSnapshot}
      receivedAt={Date.now()}
      selectedWorkspaceId="all"
      onSelectWorkspace={() => undefined}
    />,
  );

  const tabs = screen.getByRole("navigation", { name: "Workspace 筛选" });
  Object.defineProperty(tabs, "scrollLeft", { value: 0, writable: true });

  expect(fireEvent.wheel(tabs, { deltaX: 0, deltaY: 48 })).toBe(false);
  expect(tabs.scrollLeft).toBe(48);

  view.unmount();
});

it("shows the no-Snapshot state while connecting", () => {
  const view = render(
    <Hud
      connection={{
        status: "connecting",
        detail: "正在读取 Herdr",
        stale: false,
        serverVersion: null,
      }}
      snapshot={null}
      receivedAt={Date.now()}
      selectedWorkspaceId="all"
      onSelectWorkspace={() => undefined}
    />,
  );

  expect(screen.getByText("正在读取 Herdr", { selector: "strong" })).toBeTruthy();
  expect(screen.queryByRole("article")).toBeNull();

  view.unmount();
});

it("shows an empty state for a Workspace with no Agents", () => {
  const view = render(
    <Hud
      connection={{ ...staleConnection, status: "connected", stale: false }}
      snapshot={{ ...snapshot, agents: [] }}
      receivedAt={Date.now()}
      selectedWorkspaceId="all"
      onSelectWorkspace={() => undefined}
    />,
  );

  expect(screen.getByText("这个 Workspace 还没有 Agent。")).toBeTruthy();
  expect(screen.queryByRole("article")).toBeNull();

  view.unmount();
});
