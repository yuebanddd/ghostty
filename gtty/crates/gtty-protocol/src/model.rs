use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Peer {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientEnvelope {
    Handshake {
        request_id: String,
        protocol: ProtocolVersion,
        client: Peer,
        #[serde(default)]
        capabilities: BTreeSet<String>,
    },
    Request {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        method: String,
        #[serde(default)]
        params: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerEnvelope {
    Handshake {
        request_id: String,
        accepted: bool,
        protocol: ProtocolVersion,
        daemon: Peer,
        #[serde(default)]
        capabilities: BTreeSet<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<IpcError>,
    },
    Response {
        request_id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<IpcError>,
    },
    Event {
        event_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(flatten)]
        event: EventPayload,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum EventPayload {
    #[serde(rename = "daemon.ready")]
    DaemonReady { pid: u32 },
    #[serde(rename = "task.state_changed")]
    TaskStateChanged {
        previous: Option<TaskState>,
        current: TaskState,
    },
    #[serde(rename = "diagnostic")]
    Diagnostic {
        level: DiagnosticLevel,
        code: String,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    WaitingForInput,
    WaitingForApproval,
    Reviewing,
    Failed,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub session_id: Option<String>,
    pub state: TaskState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaemonSnapshot {
    pub protocol: ProtocolVersion,
    pub uptime_ms: u64,
    pub tasks: Vec<TaskSnapshot>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    IncompatibleVersion,
    NotHandshaken,
    Timeout,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl IpcError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}
