use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtty_protocol::{
    ClientEnvelope, DaemonSnapshot, EventPayload, IPC_V0, Peer, ServerEnvelope, capability,
    decode_frame, encode_frame, method,
};
use gttyd::{Daemon, bind_socket};

#[test]
fn unix_client_handshakes_receives_event_and_requests_snapshot() {
    let socket_path = unique_socket_path();
    let listener = bind_socket(&socket_path).expect("daemon socket must bind");
    let parent_mode = fs::metadata(socket_path.parent().expect("socket must have a parent"))
        .expect("socket parent metadata must exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(parent_mode, 0o700);
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

#[test]
fn daemon_accepts_a_second_client_while_the_first_is_connected() {
    let socket_path = unique_socket_path();
    let mut daemon = ChildGuard::spawn(&socket_path);
    wait_for_socket(&socket_path, &mut daemon.child);

    let (first_client, mut first_reader) = connect_client(&socket_path);
    send_handshake(&first_client, "first");
    assert_handshake_accepted(read_server_message(&mut first_reader));
    assert!(matches!(
        read_server_message(&mut first_reader),
        ServerEnvelope::Event {
            event: EventPayload::DaemonReady { .. },
            ..
        }
    ));

    let (second_client, mut second_reader) = connect_client(&socket_path);
    send_handshake(&second_client, "second");
    assert_handshake_accepted(read_server_message(&mut second_reader));
    assert!(matches!(
        read_server_message(&mut second_reader),
        ServerEnvelope::Event {
            event: EventPayload::DaemonReady { .. },
            ..
        }
    ));

    drop(second_reader);
    drop(second_client);
    drop(first_reader);
    drop(first_client);
    daemon.stop();
    remove_socket_tree(&socket_path).expect("test socket tree must be removed");
}

#[test]
fn bind_socket_preserves_an_existing_parent_mode() {
    let socket_path = unique_socket_path();
    let parent = socket_path.parent().expect("socket must have a parent");
    fs::create_dir_all(parent).expect("test parent must be created");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o750))
        .expect("test parent mode must be set");

    let error = bind_socket(&socket_path).expect_err("a shared parent must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    let mode = fs::metadata(parent)
        .expect("test parent metadata must exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o750);
    fs::remove_dir(parent).expect("test parent must be removed");
}

struct ChildGuard {
    child: Child,
    socket_path: PathBuf,
}

impl ChildGuard {
    fn spawn(socket_path: &Path) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_gttyd"))
            .arg("--socket")
            .arg(socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("gttyd process must start");
        Self {
            child,
            socket_path: socket_path.to_path_buf(),
        }
    }

    fn stop(&mut self) {
        if self
            .child
            .try_wait()
            .expect("daemon status must be readable")
            .is_none()
        {
            self.child.kill().expect("daemon process must stop");
        }
        self.child.wait().expect("daemon process must be reaped");
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        let _ = remove_socket_tree(&self.socket_path);
    }
}

fn wait_for_socket(socket_path: &Path, daemon: &mut Child) {
    for _ in 0..100 {
        if socket_path.exists() {
            return;
        }
        if let Some(status) = daemon.try_wait().expect("daemon status must be readable") {
            panic!("gttyd exited before creating its socket: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("gttyd did not create its socket within one second");
}

fn connect_client(socket_path: &Path) -> (UnixStream, BufReader<UnixStream>) {
    let stream = UnixStream::connect(socket_path).expect("client must connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("client read timeout must apply");
    let reader = BufReader::new(stream.try_clone().expect("client stream must clone"));
    (stream, reader)
}

fn send_handshake(mut client: &UnixStream, suffix: &str) {
    let handshake = ClientEnvelope::Handshake {
        request_id: format!("req-handshake-{suffix}"),
        protocol: IPC_V0,
        client: Peer {
            name: format!("gtty-test-{suffix}"),
            version: "0.1.0".into(),
        },
        capabilities: [capability::STATE_SNAPSHOT.to_owned()]
            .into_iter()
            .collect(),
    };
    client
        .write_all(&encode_frame(&handshake).unwrap())
        .expect("handshake must send");
}

fn assert_handshake_accepted(response: ServerEnvelope) {
    match response {
        ServerEnvelope::Handshake {
            accepted, error, ..
        } => {
            assert!(accepted);
            assert!(error.is_none());
        }
        other => panic!("expected handshake response, got {other:?}"),
    }
}

fn read_server_message(reader: &mut BufReader<UnixStream>) -> ServerEnvelope {
    let mut frame = Vec::new();
    reader
        .read_until(b'\n', &mut frame)
        .expect("server response must read");
    decode_frame(&frame).expect("server response must decode")
}

fn remove_socket_tree(socket_path: &Path) -> std::io::Result<()> {
    if socket_path.exists() {
        fs::remove_file(socket_path)?;
    }
    let parent = socket_path
        .parent()
        .expect("test socket must have a parent");
    if parent.exists() {
        fs::remove_dir(parent)?;
    }
    Ok(())
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
