//! Synthetic Named Pipe server exercising the production monitor without a GUI.
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use interprocess::local_socket::traits::Listener as _;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName as _};
use interprocess::TryClone as _;
use serde_json::{json, Value};

use super::*;

#[derive(Default)]
struct Events {
    connections: Mutex<Vec<ConnectionView>>,
    snapshots: Mutex<Vec<CompanionSnapshot>>,
}

impl RuntimeEvents for Events {
    fn snapshot(&self, snapshot: CompanionSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }
    fn connection(&self, connection: ConnectionView) {
        self.connections.lock().unwrap().push(connection);
    }
}

struct Step {
    method: &'static str,
    result: Value,
    event: bool,
}

fn step(method: &'static str, result: Value) -> Step {
    Step {
        method,
        result,
        event: false,
    }
}

fn ping(protocol: u32) -> Step {
    step(
        "ping",
        json!({ "type": "pong", "version": "synthetic-test", "protocol": protocol }),
    )
}

fn snapshot(label: &str, extra_pane: bool) -> Step {
    let mut snapshot: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/session_snapshot.json"
    ))
    .unwrap();
    snapshot["workspaces"][0]["label"] = json!(label);
    if extra_pane {
        let mut agent = snapshot["agents"][0].clone();
        agent["pane_id"] = json!("synthetic-new-pane");
        snapshot["agents"].as_array_mut().unwrap().push(agent);
    }
    step(
        "session.snapshot",
        json!({ "type": "session_snapshot", "snapshot": snapshot }),
    )
}

fn subscribe(event: bool) -> Step {
    Step {
        method: "events.subscribe",
        result: json!({ "type": "subscription_started" }),
        event,
    }
}

fn serve(steps: Vec<Step>) -> (HerdrClient, thread::JoinHandle<()>) {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let endpoint = std::env::temp_dir().join(format!(
        "hc-runtime-{}-{}.sock",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let name = endpoint
        .to_string_lossy()
        .to_string()
        .to_ns_name::<GenericNamespaced>()
        .unwrap();
    let listener = ListenerOptions::new().name(name).create_sync().unwrap();
    let server = thread::spawn(move || {
        let mut subscriptions = Vec::new();
        let mut expected_panes = Vec::new();
        for step in steps {
            let mut stream = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], step.method);
            if step.method == "events.subscribe" {
                let entries = request["params"]["subscriptions"].as_array().unwrap();
                assert!(!entries
                    .iter()
                    .any(|entry| entry["type"] == "pane.output_matched"));
                let mut panes: Vec<_> = entries
                    .iter()
                    .filter(|entry| entry["type"] == "pane.agent_status_changed")
                    .map(|entry| entry["pane_id"].as_str().unwrap().to_owned())
                    .collect();
                panes.sort();
                assert_eq!(panes, expected_panes);
            }
            if step.method == "session.snapshot" {
                expected_panes = step.result["snapshot"]["agents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|agent| agent["pane_id"].as_str().unwrap().to_owned())
                    .collect();
                expected_panes.sort();
            }
            writeln!(
                stream,
                "{}",
                json!({ "id": request["id"], "result": step.result })
            )
            .unwrap();
            if step.event {
                writeln!(
                    stream,
                    "{}",
                    json!({ "event": "pane.agent_status_changed", "data": {} })
                )
                .unwrap();
            }
            if step.method == "events.subscribe" {
                subscriptions.push(stream);
            }
        }
        // Closing the held event streams exercises EOF -> disconnected in monitor_once.
    });
    (HerdrClient::with_endpoint(endpoint), server)
}

#[test]
fn subscription_gap_and_new_pane_are_reconciled_before_events_are_read() {
    let (client, server) = serve(vec![
        ping(21),
        snapshot("before subscription", false),
        subscribe(false),
        snapshot("pane created in gap", true),
        subscribe(true),
        snapshot("status changed in rebuild gap", true),
        snapshot("event refresh", true),
    ]);
    let runtime = Runtime::with_client(client);
    let events = Events::default();
    let mut failures = 4;
    let delay = runtime.monitor_once(&events, &mut failures);
    server.join().unwrap();

    assert_eq!(
        delay,
        Duration::from_millis(250),
        "a healthy subscription resets backoff"
    );
    let snapshots = events.snapshots.lock().unwrap();
    let labels: Vec<_> = snapshots
        .iter()
        .map(|s| s.workspaces[0].label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "pane created in gap",
            "status changed in rebuild gap",
            "event refresh"
        ]
    );
    let states = events.connections.lock().unwrap();
    assert_eq!(
        states.iter().map(|s| s.status).collect::<Vec<_>>(),
        [
            ConnectionStatus::Connecting,
            ConnectionStatus::Connected,
            ConnectionStatus::Disconnected,
        ]
    );
    assert!(states.last().unwrap().stale);
    assert_eq!(
        runtime.view().snapshot.unwrap().workspaces[0].label,
        "event refresh"
    );
    assert!(!runtime.is_connected());
}

#[test]
fn reconnect_replaces_stale_snapshot_and_clears_stale_while_connected() {
    let (client, server) = serve(vec![
        ping(21),
        snapshot("initial", false),
        subscribe(false),
        snapshot("first connection", false),
    ]);
    let mut runtime = Runtime::with_client(client);
    let events = Events::default();
    let mut failures = 0;
    runtime.monitor_once(&events, &mut failures);
    server.join().unwrap();
    assert!(runtime.view().connection.stale);

    let (client, server) = serve(vec![
        ping(21),
        snapshot("new initial", false),
        subscribe(false),
        snapshot("reconnected", false),
    ]);
    runtime.client = client;
    runtime.monitor_once(&events, &mut failures);
    server.join().unwrap();
    let states = events.connections.lock().unwrap();
    assert!(states[3].stale, "reconnecting preserves the last snapshot");
    assert_eq!(states[4].status, ConnectionStatus::Connected);
    assert!(!states[4].stale);
    assert_eq!(
        runtime.view().snapshot.unwrap().workspaces[0].label,
        "reconnected"
    );
}

#[test]
fn incompatible_protocol_never_reads_business_data_and_backoff_grows() {
    let (client, server) = serve(vec![ping(22)]);
    let runtime = Runtime::with_client(client);
    let events = Events::default();
    let mut failures = 0;
    assert_eq!(
        runtime.monitor_once(&events, &mut failures),
        Duration::from_millis(250)
    );
    server.join().unwrap();
    assert_eq!(
        runtime.view().connection.status,
        ConnectionStatus::Incompatible
    );
    assert!(!runtime.view().connection.stale);
    assert!(runtime.view().snapshot.is_none());
    // Server has exited; subsequent connection failures reach the capped delay.
    for millis in [500, 1000, 2000, 5000, 5000] {
        assert_eq!(
            runtime.monitor_once(&events, &mut failures),
            Duration::from_millis(millis)
        );
    }
}

#[test]
fn manual_retry_is_not_lost_before_wait_begins_and_wakes_an_existing_wait() {
    let runtime = Arc::new(Runtime::with_client(HerdrClient::with_endpoint(
        "unused".into(),
    )));
    runtime.retry();
    let start = Instant::now();
    runtime.wait_for_retry(Duration::from_secs(5), 0);
    assert!(start.elapsed() < Duration::from_secs(1));

    let waiter = Arc::clone(&runtime);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let thread = thread::spawn(move || {
        waiter.wait_for_retry(Duration::from_secs(5), 1);
        done_tx.send(()).unwrap();
    });
    runtime.retry();
    done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("retry should wake promptly even across scheduling races");
    thread.join().unwrap();
}
