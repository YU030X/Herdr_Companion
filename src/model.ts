export type AgentStatus = "blocked" | "done" | "working" | "idle" | "unknown";
export type ConnectionStatus = "connecting" | "connected" | "disconnected" | "incompatible";
export type TaskSource = "token" | "title" | "terminal-title";

export interface ConnectionView {
  status: ConnectionStatus;
  detail: string;
  stale: boolean;
  serverVersion: string | null;
}

export interface CompanionWorkspace {
  id: string;
  number: number;
  label: string;
  focused: boolean;
}

export interface CompanionAgent {
  id: string;
  terminalId: string;
  name: string;
  agentType: string | null;
  status: AgentStatus;
  workspaceId: string;
  tabId: string;
  paneId: string;
  task: string | null;
  taskSource: TaskSource | null;
  parentId: string | null;
  focused: boolean;
  focusTarget: string;
  revision: number;
  stateChangeSeq: number;
  firstObservedAt: number;
  statusObservedAt: number;
  lastEventObservedAt: number;
}

export interface CompanionSnapshot {
  version: string;
  protocol: number;
  capturedAt: number;
  focusedWorkspaceId: string | null;
  workspaces: CompanionWorkspace[];
  agents: CompanionAgent[];
}

export interface RuntimeView {
  connection: ConnectionView;
  snapshot: CompanionSnapshot | null;
}

export interface StatusMeta {
  label: string;
  icon: string;
  priority: number;
  tone: AgentStatus;
}

export const STATUS: Record<AgentStatus, StatusMeta> = {
  blocked: { label: "Blocked", icon: "!", priority: 0, tone: "blocked" },
  done: { label: "Done", icon: "✓", priority: 1, tone: "done" },
  working: { label: "Working", icon: "●", priority: 2, tone: "working" },
  idle: { label: "Idle", icon: "–", priority: 3, tone: "idle" },
  unknown: { label: "Unknown", icon: "?", priority: 4, tone: "unknown" },
};

export type StatusCounts = Record<AgentStatus, number>;

export function countStatuses(agents: CompanionAgent[]): StatusCounts {
  const counts: StatusCounts = { blocked: 0, done: 0, working: 0, idle: 0, unknown: 0 };
  for (const agent of agents) counts[agent.status] += 1;
  return counts;
}

export function agentsForWorkspace(
  agents: CompanionAgent[],
  workspaceId: string,
): CompanionAgent[] {
  const visible = workspaceId === "all"
    ? agents
    : agents.filter((agent) => agent.workspaceId === workspaceId);

  return [...visible].sort((left, right) => {
    const priority = STATUS[left.status].priority - STATUS[right.status].priority;
    if (priority !== 0) return priority;
    const observed = right.statusObservedAt - left.statusObservedAt;
    return observed || left.name.localeCompare(right.name);
  });
}

export function formatObservedDuration(
  snapshot: CompanionSnapshot,
  agent: CompanionAgent,
  elapsedSinceReceipt: number,
  stale: boolean,
): string {
  const elapsed = snapshot.capturedAt - agent.firstObservedAt + (stale ? 0 : elapsedSinceReceipt);
  const totalSeconds = Math.max(0, Math.floor(elapsed / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}
