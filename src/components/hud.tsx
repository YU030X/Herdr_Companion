import { useEffect, useMemo, useState } from "preact/hooks";
import {
  STATUS,
  agentsForWorkspace,
  countStatuses,
  formatObservedDuration,
  type CompanionSnapshot,
  type ConnectionView,
  type StatusCounts,
} from "../model";

interface HudProps {
  connection: ConnectionView;
  snapshot: CompanionSnapshot | null;
  receivedAt: number;
  selectedWorkspaceId: string;
  onSelectWorkspace: (workspaceId: string) => void;
}

export function Hud({
  connection,
  snapshot,
  receivedAt,
  selectedWorkspaceId,
  onSelectWorkspace,
}: HudProps) {
  const [now, setNow] = useState(Date.now());

  const agents = useMemo(
    () => agentsForWorkspace(snapshot?.agents ?? [], selectedWorkspaceId),
    [snapshot, selectedWorkspaceId],
  );
  const counts = useMemo(() => countStatuses(agents), [agents]);

  useEffect(() => {
    let timer: number | undefined;

    const stopTimer = () => {
      if (timer !== undefined) {
        window.clearInterval(timer);
        timer = undefined;
      }
    };
    const startTimer = () => {
      if (
        timer === undefined
        && connection.status === "connected"
        && !connection.stale
        && snapshot !== null
        && agents.length > 0
        && document.visibilityState === "visible"
      ) {
        timer = window.setInterval(() => setNow(Date.now()), 1000);
      }
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "hidden") {
        stopTimer();
      } else {
        setNow(Date.now());
        startTimer();
      }
    };

    startTimer();
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      stopTimer();
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [agents.length, connection.stale, connection.status, snapshot]);

  if (!snapshot) {
    return (
      <section class="empty-state" aria-live="polite">
        <span class="empty-icon" aria-hidden="true">{connection.status === "connecting" ? "◌" : "×"}</span>
        <strong>{connection.status === "connecting" ? "正在读取 Herdr" : "还没有可显示的 Snapshot"}</strong>
        <p>{connection.detail}</p>
      </section>
    );
  }

  const workspace = snapshot.workspaces.find((item) => item.id === selectedWorkspaceId);
  const title = workspace?.label ?? "全部工作区";
  const elapsedSinceReceipt = Math.max(0, now - receivedAt);

  return (
    <>
      <nav class="workspace-tabs" aria-label="Workspace 筛选" onWheel={handleWorkspaceWheel}>
        <WorkspaceButton
          active={selectedWorkspaceId === "all"}
          count={snapshot.agents.length}
          label="全部"
          onClick={() => onSelectWorkspace("all")}
        />
        {snapshot.workspaces.map((item) => (
          <WorkspaceButton
            key={item.id}
            active={selectedWorkspaceId === item.id}
            count={snapshot.agents.filter((agent) => agent.workspaceId === item.id).length}
            label={item.label}
            onClick={() => onSelectWorkspace(item.id)}
          />
        ))}
      </nav>

      <section class="hud-panel" aria-labelledby="workspace-title">
        <header class="hud-header">
          <div>
            <span class="section-label">Workspace</span>
            <h2 id="workspace-title">{title}</h2>
          </div>
          <StatusSummary counts={counts} total={agents.length} />
        </header>

        {connection.stale && (
          <div class="stale-banner" role="status">
            <strong>Snapshot 已过期</strong>
            <span>正在重连 Herdr，当前显示最后一次快照。</span>
          </div>
        )}

        <div class="agent-list">
          {agents.length === 0 ? (
            <div class="list-empty">这个 Workspace 还没有 Agent。</div>
          ) : (
            agents.map((agent) => (
              <AgentCard
                key={agent.id}
                agent={agent}
                duration={formatObservedDuration(
                  snapshot,
                  agent,
                  elapsedSinceReceipt,
                  connection.stale,
                )}
              />
            ))
          )}
        </div>
      </section>
    </>
  );
}

function handleWorkspaceWheel(event: WheelEvent) {
  if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;

  const container = event.currentTarget;
  if (!(container instanceof HTMLElement)) return;

  const previousScrollLeft = container.scrollLeft;
  container.scrollLeft += event.deltaY;
  if (container.scrollLeft !== previousScrollLeft) event.preventDefault();
}

interface WorkspaceButtonProps {
  active: boolean;
  count: number;
  label: string;
  onClick: () => void;
}

function WorkspaceButton({ active, count, label, onClick }: WorkspaceButtonProps) {
  return (
    <button class={active ? "workspace-tab is-active" : "workspace-tab"} type="button" onClick={onClick}>
      <span>{label}</span>
      <small>{count}</small>
    </button>
  );
}

function StatusSummary({ counts, total }: { counts: StatusCounts; total: number }) {
  if (counts.blocked > 0) return <strong class="summary blocked">! {counts.blocked} blocked</strong>;
  if (counts.done > 0) return <strong class="summary done">✓ {counts.done} done</strong>;
  if (counts.working > 0) return <strong class="summary working">● {counts.working} working</strong>;
  return <strong class="summary muted">{total} agents</strong>;
}

interface AgentCardProps {
  agent: CompanionSnapshot["agents"][number];
  duration: string;
}

function AgentCard({ agent, duration }: AgentCardProps) {
  const status = STATUS[agent.status];
  return (
    <article class={`agent-card tone-${status.tone}`}>
      <span class="status-icon" aria-hidden="true" />
      <span class="agent-copy">
        <span class="agent-title-row">
          <strong>{agent.name}</strong>
          <span class="status-label">{status.label}</span>
        </span>
        <span class={agent.task ? "agent-task" : "agent-task is-fallback"}>
          {agent.task ?? "暂无可信任务描述"}
        </span>
        <span class="agent-meta">
          <span>{agent.agentType ?? "Agent"}</span>
          <time title="Observed duration">{duration}</time>
        </span>
      </span>
    </article>
  );
}
