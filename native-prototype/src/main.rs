#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use slint::{ModelRc, SharedString, VecModel, Weak};

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
        apply_ui(&weak_for_ui, &state);
    });

    apply_ui(&window.as_weak(), &state.lock().expect("UI state poisoned"));
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
        update_connection(&state, &window, "连接中", "正在连接 Herdr", None, false);
        match connect_and_monitor(&client, &state, &window) {
            Ok(()) => failure_index = 0,
            Err(error) => update_connection(
                &state,
                &window,
                "已断开",
                &error.to_string(),
                None,
                state.lock().expect("UI state poisoned").snapshot.is_some(),
            ),
        }
        let delay = RETRY_DELAYS[failure_index.min(RETRY_DELAYS.len() - 1)];
        failure_index = (failure_index + 1).min(RETRY_DELAYS.len() - 1);
        let (generation, signal) = &*retry;
        let current = *generation.lock().expect("retry signal poisoned");
        let _ = signal
            .wait_timeout_while(
                generation.lock().expect("retry signal poisoned"),
                delay,
                |value| *value == current,
            )
            .expect("retry signal poisoned");
    }
}

fn connect_and_monitor(
    client: &HerdrClient,
    state: &SharedState,
    window: &Weak<AppWindow>,
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
        update_connection(
            state,
            window,
            "已连接",
            "Herdr 已连接",
            Some(server.protocol),
            false,
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
    apply_ui(window, &state);
}

fn update_connection(
    state: &SharedState,
    window: &Weak<AppWindow>,
    label: &str,
    detail: &str,
    protocol: Option<u32>,
    stale: bool,
) {
    let mut state = state.lock().expect("UI state poisoned");
    state.connection_label = label.to_owned();
    state.connection_detail = detail.to_owned();
    if protocol.is_some() {
        state.protocol = protocol;
    }
    state.stale = stale;
    apply_ui(window, &state);
}

fn apply_ui(window: &Weak<AppWindow>, state: &UiState) {
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
        .map(|snapshot| format_agents(snapshot, selected))
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
    let _ = slint::invoke_from_event_loop(move || {
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
    });
}

fn format_agents(snapshot: &CompanionSnapshot, selected: Option<&String>) -> String {
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
                format_duration(snapshot.captured_at.saturating_sub(agent.first_observed_at))
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
}
