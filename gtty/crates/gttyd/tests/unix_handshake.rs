use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use gtty_protocol::{
    ClientEnvelope, DaemonSnapshot, EventPayload, IPC_V0, Peer, ServerEnvelope, capability,
    decode_frame, encode_frame, method,
};
use gttyd::{Daemon, bind_socket};

#[test]
fn unix_client_handshakes_receives_event_and_requests_snapshot() {
    let socket_path = unique_socket_path();
    let listener = bind_socket(&socket_path).expect("daemon socket must bind");
    let mode = fs::metadata(&socket_path)
        .expect("socket metadata must exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("mock client must connect");
        Daemon::default()
            .serve_connection(stream)
            .expect("mock connection must complete");
    });

    let mut client = UnixStream::connect(&socket_path).expect("client must connect");
    let mut reader = BufReader::new(client.try_clone().expect("client stream must clone"));

    let handshake = ClientEnvelope::Handshake {
        request_id: "req-handshake".into(),
        protocol: IPC_V0,
        client: Peer {
            name: "gtty-test-client".into(),
            version: "0.1.0".into(),
        },
        capabilities: [
            capability::STATE_SNAPSHOT.to_owned(),
            "future.capability".to_owned(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
    };
    client
        .write_all(&encode_frame(&handshake).unwrap())
        .expect("handshake must send");

    let response = read_server_message(&mut reader);
    match response {
        ServerEnvelope::Handshake {
            accepted,
            protocol,
            capabilities,
            error,
            ..
        } => {
            assert!(accepted);
            assert_eq!(protocol, IPC_V0);
            assert!(error.is_none());
            assert!(capabilities.contains(capability::STATE_SNAPSHOT));
            assert!(!capabilities.contains("future.capability"));
        }
        other => panic!("expected handshake response, got {other:?}"),
    }

    let event = read_server_message(&mut reader);
    assert!(matches!(
        event,
        ServerEnvelope::Event {
            event: EventPayload::DaemonReady { .. },
            ..
        }
    ));

    let resume = ClientEnvelope::Request {
        request_id: "req-resume".into(),
        task_id: None,
        session_id: None,
        method: method::CONNECTION_RESUME.into(),
        params: serde_json::json!({ "after_event_id": null }),
        timeout_ms: Some(1_000),
    };
    client
        .write_all(&encode_frame(&resume).unwrap())
        .expect("resume request must send");

    match read_server_message(&mut reader) {
        ServerEnvelope::Response {
            request_id,
            ok,
            result: Some(result),
            error,
        } => {
            assert_eq!(request_id, "req-resume");
            assert!(ok);
            assert_eq!(result["resumed"], false);
            assert_eq!(result["requires_snapshot"], true);
            assert!(error.is_none());
        }
        other => panic!("expected resume response, got {other:?}"),
    }

    let snapshot = ClientEnvelope::Request {
        request_id: "req-snapshot".into(),
        task_id: None,
        session_id: None,
        method: method::STATE_SNAPSHOT.into(),
        params: serde_json::Value::Null,
        timeout_ms: Some(1_000),
    };
    client
        .write_all(&encode_frame(&snapshot).unwrap())
        .expect("snapshot request must send");

    match read_server_message(&mut reader) {
        ServerEnvelope::Response {
            request_id,
            ok,
            result,
            error,
        } => {
            assert_eq!(request_id, "req-snapshot");
            assert!(ok);
            let snapshot: DaemonSnapshot =
                serde_json::from_value(result.expect("snapshot result must exist"))
                    .expect("snapshot result must match the protocol");
            assert_eq!(snapshot.protocol, IPC_V0);
            assert!(snapshot.tasks.is_empty());
            assert!(error.is_none());
        }
        other => panic!("expected snapshot response, got {other:?}"),
    }

    drop(reader);
    drop(client);
    server.join().expect("mock daemon thread must finish");
    fs::remove_file(&socket_path).expect("test socket must be removed");
    fs::remove_dir(
        socket_path
            .parent()
            .expect("test socket must have a parent"),
    )
    .expect("test socket directory must be removed");
}

fn read_server_message(reader: &mut BufReader<UnixStream>) -> ServerEnvelope {
    let mut frame = Vec::new();
    reader
        .read_until(b'\n', &mut frame)
        .expect("server response must read");
    decode_frame(&frame).expect("server response must decode")
}

fn unique_socket_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after Unix epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("gtty-test-{}-{nonce}", std::process::id()))
        .join("gttyd.sock")
}
