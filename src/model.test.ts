import { describe, expect, it } from "vitest";
import {
  agentsForWorkspace,
  countStatuses,
  formatObservedDuration,
  type CompanionAgent,
  type CompanionSnapshot,
} from "./model";

function agent(id: string, status: CompanionAgent["status"], observedAt: number): CompanionAgent {
  return {
    id,
    terminalId: `terminal-${id}`,
    name: id,
    agentType: "Test",
    status,
    workspaceId: id === "other" ? "w2" : "w1",
    tabId: "w1:t1",
    paneId: id,
    task: null,
    taskSource: null,
    parentId: null,
    focused: false,
    focusTarget: id,
    revision: 1,
    stateChangeSeq: 1,
    firstObservedAt: 1_000,
    statusObservedAt: observedAt,
    lastEventObservedAt: observedAt,
  };
}

const agents = [
  agent("working", "working", 4_000),
  agent("older-blocked", "blocked", 2_000),
  agent("newer-blocked", "blocked", 3_000),
  agent("other", "done", 5_000),
];

describe("HUD model helpers", () => {
  it("filters by workspace and sorts attention first", () => {
    expect(agentsForWorkspace(agents, "w1").map((item) => item.id)).toEqual([
      "newer-blocked",
      "older-blocked",
      "working",
    ]);
  });

  it("counts all five statuses", () => {
    expect(countStatuses(agents)).toEqual({
      blocked: 2,
      done: 1,
      working: 1,
      idle: 0,
      unknown: 0,
    });
  });

  it("freezes observed duration for stale snapshots", () => {
    const snapshot: CompanionSnapshot = {
      version: "test",
      protocol: 21,
      capturedAt: 61_000,
      focusedWorkspaceId: "w1",
      workspaces: [],
      agents: [],
    };
    const current = agent("working", "working", 1_000);

    expect(formatObservedDuration(snapshot, current, 5_000, true)).toBe("01:00");
    expect(formatObservedDuration(snapshot, current, 5_000, false)).toBe("01:05");
  });
});
