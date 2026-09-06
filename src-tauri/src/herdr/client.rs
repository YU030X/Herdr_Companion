use std::ffi::OsStr;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::Stream;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName as _};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName as _};
use serde::Serialize;

use super::schema::{
    AgentTarget, EmptyParams, ErrorResponse, EventsSubscribeParams, Request, ResponseResult,
    SessionSnapshot, Subscription, SubscriptionEvent, SuccessResponse, WireResponse,
};

pub const EXPECTED_PROTOCOL: u32 = 21;

const LIFECYCLE_SUBSCRIPTIONS: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.metadata_updated",
    "workspace.closed",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.focused",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.updated",
    "pane.focused",
    "pane.moved",
    "pane.exited",
    "pane.agent_detected",
];

#[derive(Clone)]
pub struct HerdrClient {
    endpoint: PathBuf,
    next_id: Arc<AtomicU64>,
}

#[derive(Debug, PartialEq)]
pub struct ServerInfo {
    pub version: String,
    pub protocol: u32,
}

impl HerdrClient {
    pub fn local() -> Result<Self, ClientError> {
        Ok(Self::with_endpoint(default_endpoint()?))
    }

    pub fn with_endpoint(endpoint: PathBuf) -> Self {
        Self {
            endpoint,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub fn ping(&self) -> Result<ServerInfo, ClientError> {
        match self.request("ping", EmptyParams::default())? {
            ResponseResult::Pong { version, protocol } => Ok(ServerInfo { version, protocol }),
            result => Err(ClientError::UnexpectedResult(result.kind().to_owned())),
        }
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, ClientError> {
        match self.request("session.snapshot", EmptyParams::default())? {
            ResponseResult::SessionSnapshot { snapshot } => Ok(snapshot),
            result => Err(ClientError::UnexpectedResult(result.kind().to_owned())),
        }
    }

    pub fn focus_agent(&self, target: &str) -> Result<(), ClientError> {
        // Focus success payloads vary by Herdr version; only an error envelope fails.
        self.request("agent.focus", AgentTarget { target })?;
        Ok(())
    }

    pub fn subscribe(&self, pane_ids: &[String]) -> Result<SubscriptionReader, ClientError> {
        let mut subscriptions = LIFECYCLE_SUBSCRIPTIONS
            .iter()
            .map(|kind| Subscription {
                kind: (*kind).to_owned(),
                pane_id: None,
            })
            .collect::<Vec<_>>();
        subscriptions.extend(pane_ids.iter().map(|pane_id| Subscription {
            kind: "pane.agent_status_changed".to_owned(),
            pane_id: Some(pane_id.clone()),
        }));

        let id = self.request_id();
        let request = Request {
            id: &id,
            method: "events.subscribe",
            params: EventsSubscribeParams { subscriptions },
        };
        let mut stream = connect(&self.endpoint)?;
        write_request(&mut stream, &request)?;
        let mut reader = BufReader::new(stream);
        let response = read_response(&mut reader)?;
        let success = expect_success(response, &id)?;
        match success.result {
            ResponseResult::SubscriptionStarted => Ok(SubscriptionReader { reader }),
            result => Err(ClientError::UnexpectedResult(result.kind().to_owned())),
        }
    }

    fn request<T: Serialize>(
        &self,
        method: &str,
        params: T,
    ) -> Result<ResponseResult, ClientError> {
        let id = self.request_id();
        let request = Request {
            id: &id,
            method,
            params,
        };
        let mut stream = connect(&self.endpoint)?;
        write_request(&mut stream, &request)?;
        let mut reader = BufReader::new(stream);
        let response = read_response(&mut reader)?;
        Ok(expect_success(response, &id)?.result)
    }

    fn request_id(&self) -> String {
        let value = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("companion:{value}")
    }
}

pub struct SubscriptionReader {
    reader: BufReader<Stream>,
}

impl SubscriptionReader {
    pub fn read_event(&mut self) -> Result<SubscriptionEvent, ClientError> {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line)?;
        if read == 0 || line.trim().is_empty() {
            return Err(ClientError::ConnectionClosed);
        }
        serde_json::from_str(&line).map_err(ClientError::Json)
    }
}

impl ResponseResult {
    fn kind(&self) -> &'static str {
        match self {
            Self::Pong { .. } => "pong",
            Self::SessionSnapshot { .. } => "session_snapshot",
            Self::SubscriptionStarted => "subscription_started",
            Self::Ok => "ok",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug)]
pub enum ClientError {
    MissingAppData,
    Io(io::Error),
    Json(serde_json::Error),
    Server(ErrorResponse),
    MismatchedResponseId { expected: String, actual: String },
    UnexpectedResult(String),
    ConnectionClosed,
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAppData => write!(formatter, "Windows APPDATA is unavailable"),
            Self::Io(error) => write!(formatter, "Herdr connection failed: {error}"),
            Self::Json(error) => write!(formatter, "Herdr returned invalid JSON: {error}"),
            Self::Server(response) => write!(
                formatter,
                "Herdr error {}: {}",
                response.error.code, response.error.message
            ),
            Self::MismatchedResponseId { expected, actual } => write!(
                formatter,
                "Herdr response id mismatch: expected {expected}, received {actual}"
            ),
            Self::UnexpectedResult(kind) => {
                write!(formatter, "Herdr returned unexpected result type {kind}")
            }
            Self::ConnectionClosed => write!(formatter, "Herdr closed the event connection"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn default_endpoint() -> Result<PathBuf, ClientError> {
    endpoint_from_environment(
        std::env::var_os("HERDR_SOCKET_PATH").as_deref(),
        std::env::var_os("APPDATA").as_deref(),
    )
}

fn endpoint_from_environment(
    socket_override: Option<&OsStr>,
    app_data: Option<&OsStr>,
) -> Result<PathBuf, ClientError> {
    if let Some(path) = socket_override.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let app_data = app_data.ok_or(ClientError::MissingAppData)?;
    Ok(PathBuf::from(app_data).join("herdr").join("herdr.sock"))
}

fn connect(path: &Path) -> io::Result<Stream> {
    #[cfg(windows)]
    {
        let name = path
            .to_string_lossy()
            .to_string()
            .to_ns_name::<GenericNamespaced>()?;
        Stream::connect(name)
    }

    #[cfg(unix)]
    {
        let name = path.to_fs_name::<GenericFilePath>()?;
        Stream::connect(name)
    }
}

fn write_request<T: Serialize>(stream: &mut Stream, request: &T) -> Result<(), ClientError> {
    serde_json::to_writer(&mut *stream, request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn read_response(reader: &mut BufReader<Stream>) -> Result<WireResponse, ClientError> {
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 || line.trim().is_empty() {
        return Err(ClientError::ConnectionClosed);
    }
    serde_json::from_str(&line).map_err(ClientError::Json)
}

fn expect_success(
    response: WireResponse,
    expected_id: &str,
) -> Result<SuccessResponse, ClientError> {
    match response {
        WireResponse::Success(success) => {
            if success.id != expected_id {
                return Err(ClientError::MismatchedResponseId {
                    expected: expected_id.to_owned(),
                    actual: success.id,
                });
            }
            Ok(success)
        }
        WireResponse::Error(error) => Err(ClientError::Server(error)),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(unix, windows))]
    use interprocess::TryClone as _;
    use serde_json::json;

    use super::*;

    #[test]
    fn accepts_actual_focus_success_result_types() {
        for result in [
            json!({ "type": "unsupported" }),
            json!({
                "type": "agent_info",
                "agent": { "name": "worker", "pane_id": "pane-1" }
            }),
        ] {
            let response: WireResponse = serde_json::from_value(json!({
                "id": "companion:focus",
                "result": result,
            }))
            .expect("focus success response should parse");

            let success = expect_success(response, "companion:focus")
                .expect("any successful focus envelope should be accepted");
            assert!(matches!(success.result, ResponseResult::Unsupported));
        }
    }

    #[test]
    fn endpoint_override_wins_over_default_app_data() {
        let endpoint = endpoint_from_environment(
            Some(OsStr::new("custom-herdr.sock")),
            Some(OsStr::new("C:/Users/test/AppData/Roaming")),
        )
        .expect("endpoint");

        assert_eq!(endpoint, PathBuf::from("custom-herdr.sock"));
    }

    #[test]
    fn default_endpoint_uses_roaming_app_data() {
        let endpoint =
            endpoint_from_environment(None, Some(OsStr::new("C:/Users/test/AppData/Roaming")))
                .expect("endpoint");

        assert_eq!(
            endpoint,
            PathBuf::from("C:/Users/test/AppData/Roaming/herdr/herdr.sock")
        );
    }

    #[cfg(any(unix, windows))]
    fn focus_result_over_local_socket(result: serde_json::Value, suffix: &str) {
        focus_response_over_local_socket(json!({ "result": result }), suffix, true);
    }

    #[cfg(any(unix, windows))]
    fn focus_response_over_local_socket(
        mut response: serde_json::Value,
        suffix: &str,
        succeeds: bool,
    ) {
        use std::thread;

        use interprocess::local_socket::traits::Listener as _;
        use interprocess::local_socket::ListenerOptions;
        #[cfg(unix)]
        use interprocess::local_socket::{GenericFilePath, ToFsName as _};
        #[cfg(windows)]
        use interprocess::local_socket::{GenericNamespaced, ToNsName as _};

        let endpoint = std::env::temp_dir().join(format!(
            "herdr-companion-focus-{suffix}-{}.sock",
            std::process::id()
        ));
        #[cfg(unix)]
        let name = endpoint
            .to_fs_name::<GenericFilePath>()
            .expect("socket name");
        #[cfg(windows)]
        let name = endpoint
            .to_string_lossy()
            .to_string()
            .to_ns_name::<GenericNamespaced>()
            .expect("pipe name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("listener");

        let server = thread::spawn(move || {
            let mut stream = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone"))
                .read_line(&mut line)
                .expect("request line");
            let request: serde_json::Value = serde_json::from_str(&line).expect("request json");
            assert_eq!(request["method"], "agent.focus");
            assert_eq!(request["params"]["target"], "worker");
            response["id"] = request["id"].clone();
            writeln!(stream, "{response}").expect("response");
        });

        let client = HerdrClient::with_endpoint(endpoint);
        let result = client.focus_agent("worker");
        if succeeds {
            result.expect("successful focus envelope should be accepted");
        } else {
            assert!(
                matches!(result, Err(ClientError::Server(response)) if response.error.code == "target_not_found")
            );
        }
        server.join().expect("server thread");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn focus_accepts_unsupported_success_result() {
        focus_result_over_local_socket(json!({ "type": "unsupported" }), "unsupported");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn focus_accepts_agent_info_success_result() {
        focus_result_over_local_socket(
            json!({
                "type": "agent_info",
                "agent": { "name": "worker", "pane_id": "pane-1" }
            }),
            "agent-info",
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn focus_rejects_an_expired_target_error() {
        focus_response_over_local_socket(
            json!({ "error": { "code": "target_not_found", "message": "Synthetic target no longer exists" } }),
            "expired",
            false,
        );
    }

    #[cfg(windows)]
    #[test]
    fn exchanges_ndjson_over_windows_named_pipe() {
        use std::thread;

        use interprocess::local_socket::traits::Listener as _;
        use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName as _};

        let endpoint = std::env::temp_dir().join(format!(
            "herdr-companion-test-{}-1.sock",
            std::process::id()
        ));
        let name = endpoint
            .to_string_lossy()
            .to_string()
            .to_ns_name::<GenericNamespaced>()
            .expect("pipe name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("listener");

        let server = thread::spawn(move || {
            let mut stream = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone"))
                .read_line(&mut line)
                .expect("request line");
            let request: serde_json::Value = serde_json::from_str(&line).expect("request json");
            assert_eq!(request["method"], "ping");
            assert_eq!(request["params"], serde_json::json!({}));

            let response = serde_json::json!({
                "id": request["id"],
                "result": {
                    "type": "pong",
                    "version": "0.8.2-preview.2026-08-31-b1ff4582e968",
                    "protocol": EXPECTED_PROTOCOL
                }
            });
            writeln!(stream, "{response}").expect("response");
        });

        let client = HerdrClient::with_endpoint(endpoint);
        let server_info = client.ping().expect("ping should succeed");
        assert_eq!(server_info.protocol, EXPECTED_PROTOCOL);
        server.join().expect("server thread");
    }

    #[cfg(windows)]
    #[test]
    fn keeps_subscription_connection_for_event_lines() {
        use std::thread;

        use interprocess::local_socket::traits::Listener as _;
        use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName as _};

        let endpoint = std::env::temp_dir().join(format!(
            "herdr-companion-test-{}-2.sock",
            std::process::id()
        ));
        let name = endpoint
            .to_string_lossy()
            .to_string()
            .to_ns_name::<GenericNamespaced>()
            .expect("pipe name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("listener");

        let server = thread::spawn(move || {
            let mut stream = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone"))
                .read_line(&mut line)
                .expect("request line");
            let request: serde_json::Value = serde_json::from_str(&line).expect("request json");
            assert_eq!(request["method"], "events.subscribe");
            assert!(request["params"]["subscriptions"]
                .as_array()
                .expect("subscriptions")
                .iter()
                .any(|item| {
                    item["type"] == "pane.agent_status_changed" && item["pane_id"] == "w1:p1"
                }));

            writeln!(
                stream,
                "{}",
                serde_json::json!({
                    "id": request["id"],
                    "result": { "type": "subscription_started" }
                })
            )
            .expect("subscription ack");
            writeln!(
                stream,
                "{}",
                serde_json::json!({
                    "event": "pane.agent_status_changed",
                    "data": {
                        "pane_id": "w1:p1",
                        "workspace_id": "w1",
                        "agent_status": "blocked"
                    }
                })
            )
            .expect("event");
        });

        let client = HerdrClient::with_endpoint(endpoint);
        let mut subscription = client
            .subscribe(&["w1:p1".to_owned()])
            .expect("subscribe should succeed");
        let event = subscription.read_event().expect("event should parse");
        assert_eq!(event.event, "pane.agent_status_changed");
        assert_eq!(event.data["agent_status"], "blocked");
        server.join().expect("server thread");
    }
}
