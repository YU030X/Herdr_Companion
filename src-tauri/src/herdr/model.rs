use serde::Serialize;

use super::schema::{AgentInfo, AgentStatus, SessionSnapshot};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSnapshot {
    pub version: String,
    pub protocol: u32,
    pub captured_at: u64,
    pub focused_workspace_id: Option<String>,
    pub workspaces: Vec<CompanionWorkspace>,
    pub agents: Vec<CompanionAgent>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanionWorkspace {
    pub id: String,
    pub number: usize,
    pub label: String,
    pub focused: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanionAgent {
    pub id: String,
    pub terminal_id: String,
    pub name: String,
    pub agent_type: Option<String>,
    pub status: AgentStatus,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    pub task: Option<String>,
    pub task_source: Option<TaskSource>,
    pub parent_id: Option<String>,
    pub focused: bool,
    pub focus_target: String,
    pub revision: u64,
    pub state_change_seq: u64,
    pub first_observed_at: u64,
    pub status_observed_at: u64,
    pub last_event_observed_at: u64,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSource {
    Token,
    Title,
    TerminalTitle,
}

pub fn normalize_snapshot(snapshot: SessionSnapshot, observed_at: u64) -> CompanionSnapshot {
    reconcile_snapshot(snapshot, observed_at, None)
}

pub fn reconcile_snapshot(
    snapshot: SessionSnapshot,
    observed_at: u64,
    previous: Option<&CompanionSnapshot>,
) -> CompanionSnapshot {
    let mut agents = snapshot
        .agents
        .into_iter()
        .map(|agent| normalize_agent(agent, observed_at))
        .collect::<Vec<_>>();

    if let Some(previous) = previous {
        for agent in &mut agents {
            let Some(previous_agent) = previous.agents.iter().find(|item| item.id == agent.id)
            else {
                continue;
            };

            agent.first_observed_at = previous_agent.first_observed_at;
            if agent.status == previous_agent.status
                && agent.state_change_seq == previous_agent.state_change_seq
            {
                agent.status_observed_at = previous_agent.status_observed_at;
            }
        }
    }

    agents.sort_by(|left, right| {
        left.status
            .priority()
            .cmp(&right.status.priority())
            .then_with(|| right.status_observed_at.cmp(&left.status_observed_at))
            .then_with(|| left.name.cmp(&right.name))
    });

    CompanionSnapshot {
        version: snapshot.version,
        protocol: snapshot.protocol,
        captured_at: observed_at,
        focused_workspace_id: snapshot.focused_workspace_id,
        workspaces: snapshot
            .workspaces
            .into_iter()
            .map(|workspace| CompanionWorkspace {
                id: workspace.workspace_id,
                number: workspace.number,
                label: workspace.label,
                focused: workspace.focused,
            })
            .collect(),
        agents,
    }
}

fn normalize_agent(agent: AgentInfo, observed_at: u64) -> CompanionAgent {
    let name = first_non_empty([
        agent.name.as_deref(),
        agent.display_agent.as_deref(),
        agent.agent.as_deref(),
    ])
    .unwrap_or("Agent")
    .to_owned();

    let task_with_source = token_task(&agent)
        .map(|value| (value.to_owned(), TaskSource::Token))
        .or_else(|| {
            non_empty(agent.title.as_deref()).map(|value| (value.to_owned(), TaskSource::Title))
        })
        .or_else(|| {
            non_empty(agent.terminal_title_stripped.as_deref())
                .map(|value| (value.to_owned(), TaskSource::TerminalTitle))
        });
    let (task, task_source) = match task_with_source {
        Some((task, source)) => (Some(task), Some(source)),
        None => (None, None),
    };

    let focus_target = non_empty(agent.name.as_deref())
        .unwrap_or(agent.pane_id.as_str())
        .to_owned();

    CompanionAgent {
        id: agent.pane_id.clone(),
        terminal_id: agent.terminal_id,
        name,
        agent_type: non_empty(agent.display_agent.as_deref())
            .or_else(|| non_empty(agent.agent.as_deref()))
            .map(str::to_owned),
        status: agent.agent_status,
        workspace_id: agent.workspace_id,
        tab_id: agent.tab_id,
        pane_id: agent.pane_id,
        task,
        task_source,
        parent_id: agent
            .tokens
            .get("hc_parent")
            .and_then(|value| non_empty(Some(value.as_str())).map(str::to_owned)),
        focused: agent.focused,
        focus_target,
        revision: agent.revision,
        state_change_seq: agent.state_change_seq,
        first_observed_at: observed_at,
        status_observed_at: observed_at,
        last_event_observed_at: observed_at,
    }
}

fn token_task(agent: &AgentInfo) -> Option<&str> {
    ["hc_task", "task", "summary"].into_iter().find_map(|key| {
        agent
            .tokens
            .get(key)
            .and_then(|value| non_empty(Some(value)))
    })
}

fn first_non_empty<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    values.into_iter().find_map(non_empty)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SessionSnapshot {
        serde_json::from_str(include_str!("../../tests/fixtures/session_snapshot.json"))
            .expect("fixture should follow the Herdr schema")
    }

    #[test]
    fn normalizes_status_order_task_sources_and_focus_targets() {
        let snapshot = normalize_snapshot(fixture(), 42_000);

        let statuses = snapshot
            .agents
            .iter()
            .map(|agent| agent.status)
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec![
                AgentStatus::Blocked,
                AgentStatus::Done,
                AgentStatus::Working,
                AgentStatus::Idle,
                AgentStatus::Unknown,
            ]
        );

        let blocked = &snapshot.agents[0];
        assert_eq!(blocked.task.as_deref(), Some("Approve migration"));
        assert_eq!(blocked.task_source, Some(TaskSource::Token));
        assert_eq!(blocked.parent_id.as_deref(), Some("pane-lead"));
        assert_eq!(blocked.focus_target, "reviewer");

        let done = &snapshot.agents[1];
        assert_eq!(done.task.as_deref(), Some("Review complete"));
        assert_eq!(done.task_source, Some(TaskSource::Title));
        assert_eq!(done.focus_target, "pane-done");
    }

    #[test]
    fn treats_unrecognized_status_as_unknown_and_does_not_invent_task_or_parent() {
        let snapshot = normalize_snapshot(fixture(), 42_000);
        let unknown = snapshot.agents.last().expect("unknown agent");

        assert_eq!(unknown.status, AgentStatus::Unknown);
        assert_eq!(unknown.task, None);
        assert_eq!(unknown.task_source, None);
        assert_eq!(unknown.parent_id, None);
    }

    #[test]
    fn initial_observation_uses_local_monotonic_tick() {
        let snapshot = normalize_snapshot(fixture(), 42_000);
        let agent = &snapshot.agents[0];

        assert_eq!(snapshot.captured_at, 42_000);
        assert_eq!(agent.first_observed_at, 42_000);
        assert_eq!(agent.status_observed_at, 42_000);
        assert_eq!(agent.last_event_observed_at, 42_000);
    }

    #[test]
    fn reconciliation_preserves_first_seen_and_unchanged_status_ticks() {
        let previous = normalize_snapshot(fixture(), 10_000);
        let current = reconcile_snapshot(fixture(), 12_000, Some(&previous));
        let agent = &current.agents[0];

        assert_eq!(agent.first_observed_at, 10_000);
        assert_eq!(agent.status_observed_at, 10_000);
        assert_eq!(agent.last_event_observed_at, 12_000);
    }
}
