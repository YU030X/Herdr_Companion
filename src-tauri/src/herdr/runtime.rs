use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::client::{ClientError, HerdrClient, EXPECTED_PROTOCOL};
use super::model::{reconcile_snapshot, CompanionSnapshot};
use super::schema::SessionSnapshot;

pub const SNAPSHOT_EVENT: &str = "companion://snapshot-changed";
pub const CONNECTION_EVENT: &str = "companion://connection-changed";

trait RuntimeEvents {
    fn snapshot(&self, snapshot: CompanionSnapshot);
    fn connection(&self, connection: ConnectionView);
}

impl RuntimeEvents for AppHandle {
    fn snapshot(&self, snapshot: CompanionSnapshot) {
        let _ = self.emit(SNAPSHOT_EVENT, snapshot);
    }

    fn connection(&self, connection: ConnectionView) {
        let _ = self.emit(CONNECTION_EVENT, connection);
    }
}

const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
    Incompatible,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub status: ConnectionStatus,
    pub detail: String,
    pub stale: bool,
    pub server_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeView {
    pub connection: ConnectionView,
    pub snapshot: Option<CompanionSnapshot>,
}

pub struct Runtime {
    client: HerdrClient,
    view: RwLock<RuntimeView>,
    retry_generation: Mutex<u64>,
    retry_signal: Condvar,
    started_at: Instant,
}

impl Runtime {
    pub fn new() -> Result<Self, ClientError> {
        let client = HerdrClient::local()?;
        Ok(Self::with_client(client))
    }

    fn with_client(client: HerdrClient) -> Self {
        Self {
            client,
            view: RwLock::new(RuntimeView {
                connection: ConnectionView {
                    status: ConnectionStatus::Connecting,
                    detail: "正在连接 Herdr".to_owned(),
                    stale: false,
                    server_version: None,
                },
                snapshot: None,
            }),
            retry_generation: Mutex::new(0),
            retry_signal: Condvar::new(),
            started_at: Instant::now(),
        }
    }

    pub fn start(self: Arc<Self>, app: AppHandle) {
        std::thread::Builder::new()
            .name("herdr-monitor".to_owned())
            .spawn(move || self.monitor(app))
            .expect("failed to start Herdr monitor thread");
    }

    pub fn view(&self) -> RuntimeView {
        self.view.read().expect("runtime view poisoned").clone()
    }

    pub fn client(&self) -> &HerdrClient {
        &self.client
    }

    pub fn is_connected(&self) -> bool {
        self.view
            .read()
            .expect("runtime view poisoned")
            .connection
            .status
            == ConnectionStatus::Connected
    }

    pub fn retry(&self) {
        let mut generation = self.retry_generation.lock().expect("retry signal poisoned");
        *generation = generation.wrapping_add(1);
        self.retry_signal.notify_all();
    }

    fn monitor(&self, app: AppHandle) {
        let mut failure_index = 0;
        loop {
            // Capture before publishing Disconnected so an immediate retry is not lost.
            let generation = *self.retry_generation.lock().expect("retry signal poisoned");
            let delay = self.monitor_once(&app, &mut failure_index);
            self.wait_for_retry(delay, generation);
        }
    }

    fn monitor_once(&self, app: &impl RuntimeEvents, failure_index: &mut usize) -> Duration {
        self.publish_connection(
            app,
            ConnectionStatus::Connecting,
            "正在连接 Herdr".to_owned(),
            None,
        );

        match self.connect_and_monitor(app, failure_index) {
            Ok(()) => {}
            Err(MonitorError::Client(error)) => {
                self.publish_connection(
                    app,
                    ConnectionStatus::Disconnected,
                    error.to_string(),
                    None,
                );
            }
            Err(MonitorError::Incompatible { version, protocol }) => {
                self.publish_connection(
                    app,
                    ConnectionStatus::Incompatible,
                    format!("Herdr 协议不兼容：需要 {EXPECTED_PROTOCOL}，当前为 {protocol}"),
                    Some(version),
                );
            }
        }

        let delay = RETRY_DELAYS[(*failure_index).min(RETRY_DELAYS.len() - 1)];
        *failure_index = (*failure_index + 1).min(RETRY_DELAYS.len() - 1);
        delay
    }

    fn connect_and_monitor(
        &self,
        app: &impl RuntimeEvents,
        failure_index: &mut usize,
    ) -> Result<(), MonitorError> {
        let server = self.client.ping()?;
        if server.protocol != EXPECTED_PROTOCOL {
            return Err(MonitorError::Incompatible {
                version: server.version,
                protocol: server.protocol,
            });
        }

        let snapshot = self.client.snapshot()?;
        let mut pane_ids = agent_pane_ids(&snapshot);
        loop {
            let mut subscription = self.client.subscribe(&pane_ids)?;
            // Re-read after every subscription acknowledgement to cover changes in the gap.
            let snapshot = self.client.snapshot()?;
            let next_pane_ids = agent_pane_ids(&snapshot);
            self.publish_snapshot(app, snapshot);
            if next_pane_ids != pane_ids {
                pane_ids = next_pane_ids;
                continue;
            }
            *failure_index = 0;
            self.publish_connection(
                app,
                ConnectionStatus::Connected,
                "Herdr 已连接".to_owned(),
                Some(server.version.clone()),
            );
            loop {
                let event = subscription.read_event()?;
                let _ = (event.event, event.data);

                let snapshot = self.client.snapshot()?;
                let next_pane_ids = agent_pane_ids(&snapshot);
                self.publish_snapshot(app, snapshot);
                if next_pane_ids != pane_ids {
                    pane_ids = next_pane_ids;
                    break;
                }
            }
        }
    }

    fn publish_snapshot(&self, app: &impl RuntimeEvents, raw: SessionSnapshot) {
        let observed_at = self.monotonic_millis();
        let previous = self
            .view
            .read()
            .expect("runtime view poisoned")
            .snapshot
            .clone();
        let snapshot = reconcile_snapshot(raw, observed_at, previous.as_ref());

        self.view.write().expect("runtime view poisoned").snapshot = Some(snapshot.clone());
        app.snapshot(snapshot);
    }

    fn publish_connection(
        &self,
        app: &impl RuntimeEvents,
        status: ConnectionStatus,
        detail: String,
        server_version: Option<String>,
    ) {
        let connection = {
            let mut view = self.view.write().expect("runtime view poisoned");
            let connection = ConnectionView {
                status,
                detail,
                stale: status != ConnectionStatus::Connected && view.snapshot.is_some(),
                server_version: server_version.or_else(|| view.connection.server_version.clone()),
            };
            view.connection = connection.clone();
            connection
        };
        app.connection(connection);
    }

    fn monotonic_millis(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    fn wait_for_retry(&self, duration: Duration, current: u64) {
        let generation = self.retry_generation.lock().expect("retry signal poisoned");
        let _ = self
            .retry_signal
            .wait_timeout_while(generation, duration, |value| *value == current)
            .expect("retry signal poisoned");
    }
}

fn agent_pane_ids(snapshot: &SessionSnapshot) -> Vec<String> {
    let mut pane_ids = snapshot
        .agents
        .iter()
        .map(|agent| agent.pane_id.clone())
        .collect::<Vec<_>>();
    pane_ids.sort();
    pane_ids.dedup();
    pane_ids
}

#[derive(Debug)]
enum MonitorError {
    Client(ClientError),
    Incompatible { version: String, protocol: u32 },
}

impl From<ClientError> for MonitorError {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_pane_ids_are_stable_and_unique() {
        let snapshot: SessionSnapshot =
            serde_json::from_str(include_str!("../../tests/fixtures/session_snapshot.json"))
                .expect("fixture");

        assert_eq!(
            agent_pane_ids(&snapshot),
            vec![
                "pane-blocked",
                "pane-done",
                "pane-idle",
                "pane-unknown",
                "pane-working",
            ]
        );
    }
}

#[cfg(all(test, windows))]
mod connection_tests;
