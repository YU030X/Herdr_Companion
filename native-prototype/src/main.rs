#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use slint::{ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

#[allow(dead_code)]
#[path = "../../src-tauri/src/herdr/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "../../src-tauri/src/herdr/model.rs"]
mod model;
#[allow(dead_code)]
#[path = "../../src-tauri/src/herdr/schema.rs"]
mod schema;

slint::include_modules!();

use client::{ClientError, HerdrClient};
use model::{reconcile_snapshot, CompanionSnapshot};
use schema::{AgentStatus, SessionSnapshot};

const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

#[derive(Clone, Debug, Default)]
struct UiState {
    connection_label: String,
    connection_detail: String,
    protocol: Option<u32>,
    stale: bool,
    snapshot: Option<CompanionSnapshot>,
    selected_workspace_id: Option<String>,
}

type SharedState = Arc<Mutex<UiState>>;
type RetrySignal = Arc<(Mutex<u64>, Condvar)>;

#[derive(Clone)]
struct UiData {
    connection_label: String,
    connection_detail: String,
    protocol_label: String,
    workspace_title: String,
    workspace_tabs: Vec<String>,
    selected_workspace: i32,
    agents_text: String,
    stale: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;
    let state = Arc::new(Mutex::new(UiState {
        connection_label: "连接中".to_owned(),
        connection_detail: "正在读取 Herdr".to_owned(),
        ..UiState::default()
    }));
    let retry_signal = Arc::new((Mutex::new(0_u64), Condvar::new()));
    let client = HerdrClient::local()?;

    let retry_for_ui = Arc::clone(&retry_signal);
    window.on_retry_clicked(move || {
        let (generation, signal) = &*retry_for_ui;
        *generation.lock().expect("retry signal poisoned") += 1;
        signal.notify_all();
    });

    let state_for_ui = Arc::clone(&state);
    let weak_for_ui = window.as_weak();
    window.on_workspace_clicked(move |index| {
        let mut state = state_for_ui.lock().expect("UI state poisoned");
        let selected_workspace_id = state.snapshot.as_ref().and_then(|snapshot| {
            if index == 0 {
                None
            } else {
                snapshot
                    .workspaces
                    .get(index as usize - 1)
                    .map(|workspace| workspace.id.clone())
            }
        });
        state.selected_workspace_id = selected_workspace_id;
        apply_ui(&weak_for_ui, &mut state);
    });

    {
        let mut state = state.lock().expect("UI state poisoned");
        apply_ui(&window.as_weak(), &mut state);
    }

    let refresh_state = Arc::clone(&state);
    let refresh_window = window.as_weak();
    let refresh_timer = Timer::default();
    refresh_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let mut state = refresh_state.lock().expect("UI state poisoned");
        if state.snapshot.is_some() && state.connection_label == "已连接" && !state.stale {
            apply_ui(&refresh_window, &mut state);
        }
    });

    spawn_monitor(client, Arc::clone(&state), window.as_weak(), retry_signal);
    window.run()?;
    Ok(())
}

fn spawn_monitor(
    client: HerdrClient,
    state: SharedState,
    window: Weak<AppWindow>,
    retry: RetrySignal,
) {
    thread::Builder::new()
        .name("herdr-native-prototype".to_owned())
        .spawn(move || monitor(client, state, window, retry))
        .expect("failed to start native monitor");
}

fn monitor(client: HerdrClient, state: SharedState, window: Weak<AppWindow>, retry: RetrySignal) {
    let mut failure_index = 0_usize;
    loop {
        // Capture before publishing Connecting/Disconnected so an immediate retry is not lost.
        let generation = retry.0.lock().expect("retry signal poisoned");
        let generation = *generation;
        update_connection(&state, &window, "连接中", "正在连接 Herdr", None);
        if let Err(error) = connect_and_monitor(&client, &state, &window, &mut failure_index) {
            update_connection(&state, &window, "已断开", &error.to_string(), None);
        }
        let delay = RETRY_DELAYS[failure_index.min(RETRY_DELAYS.len() - 1)];
        failure_index = (failure_index + 1).min(RETRY_DELAYS.len() - 1);
        let (_, signal) = &*retry;
        let _ = signal
            .wait_timeout_while(
                retry.0.lock().expect("retry signal poisoned"),
                delay,
                |value| *value == generation,
            )
            .expect("retry signal poisoned");
    }
}

fn connect_and_monitor(
    client: &HerdrClient,
    state: &SharedState,
    window: &Weak<AppWindow>,
    failure_index: &mut usize,
) -> Result<(), ClientError> {
    let server = client.ping()?;
    let snapshot = client.snapshot()?;
    let mut pane_ids = agent_pane_ids(&snapshot);
    loop {
        let mut subscription = client.subscribe(&pane_ids)?;
        let snapshot = client.snapshot()?;
        let next_pane_ids = agent_pane_ids(&snapshot);
        update_snapshot(state, window, snapshot, Some(server.protocol), false);
        if next_pane_ids != pane_ids {
            pane_ids = next_pane_ids;
            continue;
        }
        *failure_index = 0;
        update_connection(
            state,
            window,
            "已连接",
            "Herdr 已连接",
            Some(server.protocol),
        );
        loop {
            let _event = subscription.read_event()?;
            let snapshot = client.snapshot()?;
            let next_pane_ids = agent_pane_ids(&snapshot);
            update_snapshot(state, window, snapshot, Some(server.protocol), false);
            if next_pane_ids != pane_ids {
                pane_ids = next_pane_ids;
                break;
            }
        }
    }
}

fn update_snapshot(
    state: &SharedState,
    window: &Weak<AppWindow>,
    raw: SessionSnapshot,
    protocol: Option<u32>,
    stale: bool,
) {
    let mut state = state.lock().expect("UI state poisoned");
    let previous = state.snapshot.as_ref();
    state.snapshot = Some(reconcile_snapshot(raw, monotonic_millis(), previous));
    state.protocol = protocol;
    state.stale = stale;
    apply_ui(window, &mut state);
}

fn update_connection(
    state: &SharedState,
    window: &Weak<AppWindow>,
    label: &str,
    detail: &str,
    protocol: Option<u32>,
) {
    let mut state = state.lock().expect("UI state poisoned");
    state.connection_label = label.to_owned();
    state.connection_detail = detail.to_owned();
    state.protocol = protocol;
    state.stale = connection_is_stale(label, state.snapshot.is_some());
    apply_ui(window, &mut state);
}

fn apply_ui(window: &Weak<AppWindow>, state: &mut UiState) {
    normalize_workspace_selection(state);
    let view = state.snapshot.as_ref();
    let mut tabs = vec![SharedString::from("全部")];
    if let Some(snapshot) = view {
        tabs.extend(
            snapshot
                .workspaces
                .iter()
                .map(|workspace| SharedString::from(workspace.label.as_str())),
        );
    }
    let selected = state.selected_workspace_id.as_ref();
    let selected_index = selected
        .and_then(|id| {
            view.and_then(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .position(|workspace| &workspace.id == id)
                    .map(|index| index + 1)
            })
        })
        .unwrap_or(0);
    let workspace_title = selected
        .and_then(|id| {
            view.and_then(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| &workspace.id == id)
                    .map(|workspace| workspace.label.clone())
            })
        })
        .unwrap_or_else(|| "全部工作区".to_owned());
    let agents_text = view
        .map(|snapshot| {
            let observed_at = if state.stale {
                snapshot.captured_at
            } else {
                monotonic_millis()
            };
            format_agents(snapshot, selected, observed_at)
        })
        .unwrap_or_else(|| "等待 Snapshot…".to_owned());
    let data = UiData {
        connection_label: state.connection_label.clone(),
        connection_detail: state.connection_detail.clone(),
        protocol_label: format!(
            "Protocol {}",
            state
                .protocol
                .map_or("–".to_owned(), |value| value.to_string())
        ),
        workspace_title,
        workspace_tabs: tabs.into_iter().map(|value| value.to_string()).collect(),
        selected_workspace: selected_index as i32,
        agents_text,
        stale: state.stale,
    };
    let window = window.clone();
    if let Err(error) = slint::invoke_from_event_loop(move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        window.set_connection_label(data.connection_label.into());
        window.set_connection_detail(data.connection_detail.into());
        window.set_protocol_label(data.protocol_label.into());
        window.set_workspace_title(data.workspace_title.into());
        window.set_workspace_tabs(ModelRc::new(VecModel::from(
            data.workspace_tabs
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        window.set_selected_workspace(data.selected_workspace);
        window.set_agents_text(data.agents_text.into());
        window.set_stale(data.stale);
    }) {
        eprintln!("native prototype UI update skipped: {error}");
    }
}

fn connection_is_stale(label: &str, has_snapshot: bool) -> bool {
    label != "已连接" && has_snapshot
}

fn normalize_workspace_selection(state: &mut UiState) {
    let selected_workspace_id = state.selected_workspace_id.clone().filter(|id| {
        state.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .workspaces
                .iter()
                .any(|workspace| &workspace.id == id)
        })
    });
    state.selected_workspace_id = selected_workspace_id;
}

fn format_agents(
    snapshot: &CompanionSnapshot,
    selected: Option<&String>,
    observed_at: u64,
) -> String {
    let lines = snapshot
        .agents
        .iter()
        .filter(|agent| selected.is_none_or(|id| &agent.workspace_id == id))
        .map(|agent| {
            format!(
                "{}  {}\n{}\n{} · {}",
                status_icon(agent.status),
                agent.name,
                agent.task.as_deref().unwrap_or("暂无可信任务描述"),
                agent.agent_type.as_deref().unwrap_or("Agent"),
                format_duration(observed_at.saturating_sub(agent.first_observed_at))
            )
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "这个 Workspace 还没有 Agent。".to_owned()
    } else {
        lines.join("\n\n")
    }
}

fn status_icon(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Blocked => "! Blocked",
        AgentStatus::Done => "✓ Done",
        AgentStatus::Working => "● Working",
        AgentStatus::Idle => "○ Idle",
        AgentStatus::Unknown => "? Unknown",
    }
}

fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn agent_pane_ids(snapshot: &SessionSnapshot) -> Vec<String> {
    let mut ids = snapshot
        .agents
        .iter()
        .map(|agent| agent.pane_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn monotonic_millis() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_statuses_and_observed_duration_for_the_hud() {
        assert_eq!(status_icon(AgentStatus::Blocked), "! Blocked");
        assert_eq!(status_icon(AgentStatus::Done), "✓ Done");
        assert_eq!(status_icon(AgentStatus::Working), "● Working");
        assert_eq!(status_icon(AgentStatus::Idle), "○ Idle");
        assert_eq!(status_icon(AgentStatus::Unknown), "? Unknown");
        assert_eq!(format_duration(3_723_000), "62:03");
    }

    #[test]
    fn cached_snapshot_is_stale_until_connection_is_restored() {
        assert!(connection_is_stale("连接中", true));
        assert!(connection_is_stale("已断开", true));
        assert!(!connection_is_stale("连接中", false));
        assert!(!connection_is_stale("已连接", true));
    }

    #[test]
    fn missing_workspace_selection_is_cleared_before_filtering() {
        let mut state = UiState {
            snapshot: Some(CompanionSnapshot {
                version: "test".to_owned(),
                protocol: 22,
                captured_at: 100,
                focused_workspace_id: None,
                workspaces: vec![model::CompanionWorkspace {
                    id: "workspace-present".to_owned(),
                    number: 1,
                    label: "Present".to_owned(),
                    focused: false,
                }],
                agents: Vec::new(),
            }),
            selected_workspace_id: Some("workspace-gone".to_owned()),
            ..UiState::default()
        };

        normalize_workspace_selection(&mut state);

        assert_eq!(state.selected_workspace_id, None);
    }

    #[test]
    fn connected_duration_uses_the_current_refresh_time() {
        let snapshot = CompanionSnapshot {
            version: "test".to_owned(),
            protocol: 22,
            captured_at: 1_000,
            focused_workspace_id: None,
            workspaces: Vec::new(),
            agents: vec![model::CompanionAgent {
                id: "agent-1".to_owned(),
                terminal_id: "terminal-1".to_owned(),
                name: "Agent".to_owned(),
                agent_type: Some("worker".to_owned()),
                status: AgentStatus::Working,
                workspace_id: "workspace-1".to_owned(),
                tab_id: "tab-1".to_owned(),
                pane_id: "pane-1".to_owned(),
                task: None,
                task_source: None,
                parent_id: None,
                focused: false,
                focus_target: "pane-1".to_owned(),
                revision: 1,
                state_change_seq: 1,
                first_observed_at: 1_000,
                status_observed_at: 1_000,
                last_event_observed_at: 1_000,
            }],
        };

        assert!(format_agents(&snapshot, None, 6_000).ends_with("worker · 00:05"));
    }
}
