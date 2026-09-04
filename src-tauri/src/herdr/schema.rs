use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Request<'a, T> {
    pub id: &'a str,
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WireResponse {
    Success(SuccessResponse),
    Error(ErrorResponse),
}

#[derive(Debug, Deserialize)]
pub struct SuccessResponse {
    pub id: String,
    pub result: ResponseResult,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseResult {
    Pong {
        version: String,
        protocol: u32,
    },
    SessionSnapshot {
        snapshot: SessionSnapshot,
    },
    SubscriptionStarted,
    Ok,
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub id: String,
    pub error: ErrorBody,
}

#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Default, Debug, Serialize)]
pub struct EmptyParams {}

#[derive(Debug, Serialize)]
pub struct AgentTarget<'a> {
    pub target: &'a str,
}

#[derive(Debug, Serialize)]
pub struct EventsSubscribeParams {
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Serialize)]
pub struct Subscription {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionEvent {
    pub event: String,
    pub data: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Blocked,
    Done,
    Working,
    Idle,
    #[serde(other)]
    Unknown,
}

impl AgentStatus {
    pub const fn priority(self) -> u8 {
        match self {
            Self::Blocked => 0,
            Self::Done => 1,
            Self::Working => 2,
            Self::Idle => 3,
            Self::Unknown => 4,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SessionSnapshot {
    pub version: String,
    pub protocol: u32,
    pub workspaces: Vec<WorkspaceInfo>,
    pub tabs: Vec<Value>,
    pub panes: Vec<Value>,
    pub layouts: Vec<Value>,
    pub agents: Vec<AgentInfo>,
    pub focused_workspace_id: Option<String>,
    pub focused_tab_id: Option<String>,
    pub focused_pane_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub pane_count: usize,
    pub tab_count: usize,
    pub active_tab_id: String,
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
    pub worktree: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AgentInfo {
    pub terminal_id: String,
    pub agent_status: AgentStatus,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    pub focused: bool,
    pub revision: u64,
    #[serde(default)]
    pub state_change_seq: u64,
    pub name: Option<String>,
    pub agent: Option<String>,
    pub display_agent: Option<String>,
    pub title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    #[serde(default)]
    pub state_labels: HashMap<String, String>,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn serializes_newline_protocol_request_body() {
        let request = Request {
            id: "companion:1",
            method: "session.snapshot",
            params: EmptyParams::default(),
        };

        assert_eq!(
            serde_json::to_value(request).expect("request should serialize"),
            json!({
                "id": "companion:1",
                "method": "session.snapshot",
                "params": {}
            })
        );
    }

    #[test]
    fn parses_success_and_error_envelopes() {
        let success: WireResponse = serde_json::from_value(json!({
            "id": "companion:ping",
            "result": {
                "type": "pong",
                "version": "0.8.2-preview.2026-08-31-b1ff4582e968",
                "protocol": 21,
                "future_field": true
            }
        }))
        .expect("pong should parse");
        match success {
            WireResponse::Success(SuccessResponse {
                id,
                result: ResponseResult::Pong { version, protocol },
            }) => {
                assert_eq!(id, "companion:ping");
                assert_eq!(version, "0.8.2-preview.2026-08-31-b1ff4582e968");
                assert_eq!(protocol, 21);
            }
            _ => panic!("expected pong response"),
        }

        let error: WireResponse = serde_json::from_value(json!({
            "id": "companion:focus",
            "error": {
                "code": "agent_not_found",
                "message": "Agent was not found"
            }
        }))
        .expect("error should parse");
        match error {
            WireResponse::Error(ErrorResponse { id, error }) => {
                assert_eq!(id, "companion:focus");
                assert_eq!(error.code, "agent_not_found");
                assert_eq!(error.message, "Agent was not found");
            }
            _ => panic!("expected error response"),
        }
    }
}
