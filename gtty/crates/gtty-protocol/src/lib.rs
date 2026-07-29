//! Versioned, transport-independent messages for GTTY local IPC.

mod frame;
mod model;
mod negotiate;

pub use frame::{FrameError, MAX_FRAME_BYTES, decode_frame, encode_frame};
pub use model::{
    ClientEnvelope, DaemonSnapshot, DiagnosticLevel, ErrorCode, EventPayload, IpcError, Peer,
    ProtocolVersion, ServerEnvelope, TaskSnapshot, TaskState,
};
pub use negotiate::{NegotiatedProtocol, negotiate};

/// Current protocol version implemented by this workspace.
pub const IPC_V0: ProtocolVersion = ProtocolVersion::new(0, 1);

/// Capability names are open strings so newer peers can add values without
/// breaking older deserializers.
pub mod capability {
    pub const STATE_SNAPSHOT: &str = "state.snapshot";
    pub const CONNECTION_RESUME: &str = "connection.resume";
    pub const TASK_STATE_EVENT: &str = "event.task_state_changed";
    pub const DIAGNOSTIC_EVENT: &str = "event.diagnostic";
}

/// Request method names supported by IPC v0.
pub mod method {
    pub const STATE_SNAPSHOT: &str = "state.snapshot";
    pub const CONNECTION_RESUME: &str = "connection.resume";
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::*;

    #[test]
    fn examples_are_valid_protocol_messages() {
        let client: ClientEnvelope = serde_json::from_str(include_str!(
            "../../../protocol/v0/examples/handshake-request.json"
        ))
        .expect("handshake request fixture must deserialize");
        assert!(matches!(client, ClientEnvelope::Handshake { .. }));

        let server: ServerEnvelope = serde_json::from_str(include_str!(
            "../../../protocol/v0/examples/handshake-response.json"
        ))
        .expect("handshake response fixture must deserialize");
        assert!(matches!(server, ServerEnvelope::Handshake { .. }));

        let event: ServerEnvelope = serde_json::from_str(include_str!(
            "../../../protocol/v0/examples/daemon-ready-event.json"
        ))
        .expect("event fixture must deserialize");
        assert!(matches!(event, ServerEnvelope::Event { .. }));

        let request: ClientEnvelope = serde_json::from_str(include_str!(
            "../../../protocol/v0/examples/state-snapshot-request.json"
        ))
        .expect("snapshot request fixture must deserialize");
        assert!(matches!(request, ClientEnvelope::Request { .. }));
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compatibility() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../../protocol/v0/examples/handshake-request.json"
        ))
        .expect("fixture must be valid JSON");
        value
            .as_object_mut()
            .expect("fixture must be an object")
            .insert("future_field".into(), Value::Bool(true));

        let message = serde_json::from_value::<ClientEnvelope>(value);
        assert!(message.is_ok());
    }

    #[test]
    fn incompatible_major_version_is_rejected() {
        let client_capabilities = BTreeSet::new();
        let daemon_capabilities = BTreeSet::new();
        let result = negotiate(
            ProtocolVersion::new(1, 0),
            &client_capabilities,
            IPC_V0,
            &daemon_capabilities,
        );

        let error = result.expect_err("major version mismatch must fail");
        assert_eq!(error.code, ErrorCode::IncompatibleVersion);
        assert!(!error.retryable);
    }
}
