//! Minimal GTTY daemon transport and IPC v0 handshake.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gtty_protocol::{
    ClientEnvelope, DaemonSnapshot, ErrorCode, EventPayload, IPC_V0, IpcError, MAX_FRAME_BYTES,
    Peer, ServerEnvelope, capability, decode_frame, encode_frame, method, negotiate,
};
use serde_json::json;

pub const DAEMON_NAME: &str = "gttyd";
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Daemon {
    started_at: Instant,
    capabilities: BTreeSet<String>,
}

impl Default for Daemon {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            capabilities: [
                capability::STATE_SNAPSHOT,
                capability::CONNECTION_RESUME,
                capability::TASK_STATE_EVENT,
                capability::DIAGNOSTIC_EVENT,
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

impl Daemon {
    #[must_use]
    pub fn capabilities(&self) -> &BTreeSet<String> {
        &self.capabilities
    }

    #[must_use]
    pub fn snapshot(&self) -> DaemonSnapshot {
        let uptime_ms = u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        DaemonSnapshot {
            protocol: IPC_V0,
            uptime_ms,
            tasks: Vec::new(),
        }
    }

    pub fn serve_connection(&self, stream: UnixStream) -> io::Result<()> {
        let reader = stream.try_clone()?;
        self.serve_io(io::BufReader::new(reader), stream)
    }

    pub fn serve_io<R: BufRead, W: Write>(&self, mut reader: R, mut writer: W) -> io::Result<()> {
        let mut handshaken = false;
        let mut frame = Vec::new();

        loop {
            frame.clear();
            let bytes_read = reader
                .by_ref()
                .take((MAX_FRAME_BYTES + 2) as u64)
                .read_until(b'\n', &mut frame)?;
            if bytes_read == 0 {
                return Ok(());
            }
            if frame.len() > MAX_FRAME_BYTES + 1 {
                write_message(
                    &mut writer,
                    &error_response(
                        "unknown",
                        ErrorCode::InvalidRequest,
                        "IPC frame exceeds the 1 MiB limit",
                        false,
                    ),
                )?;
                return Ok(());
            }

            let message = match decode_frame::<ClientEnvelope>(&frame) {
                Ok(message) => message,
                Err(error) => {
                    write_message(
                        &mut writer,
                        &error_response(
                            "unknown",
                            ErrorCode::InvalidRequest,
                            error.to_string(),
                            false,
                        ),
                    )?;
                    continue;
                }
            };

            match message {
                ClientEnvelope::Handshake {
                    request_id,
                    protocol,
                    capabilities,
                    ..
                } => {
                    if handshaken {
                        write_message(
                            &mut writer,
                            &error_response(
                                request_id,
                                ErrorCode::InvalidRequest,
                                "connection is already handshaken",
                                false,
                            ),
                        )?;
                        continue;
                    }

                    match negotiate(protocol, &capabilities, IPC_V0, &self.capabilities) {
                        Ok(negotiated) => {
                            handshaken = true;
                            write_message(
                                &mut writer,
                                &ServerEnvelope::Handshake {
                                    request_id,
                                    accepted: true,
                                    protocol: negotiated.version,
                                    daemon: daemon_peer(),
                                    capabilities: negotiated.capabilities,
                                    error: None,
                                },
                            )?;
                            write_message(
                                &mut writer,
                                &ServerEnvelope::Event {
                                    event_id: format!(
                                        "evt-{}",
                                        NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed)
                                    ),
                                    task_id: None,
                                    session_id: None,
                                    event: EventPayload::DaemonReady {
                                        pid: std::process::id(),
                                    },
                                },
                            )?;
                        }
                        Err(error) => {
                            write_message(
                                &mut writer,
                                &ServerEnvelope::Handshake {
                                    request_id,
                                    accepted: false,
                                    protocol: IPC_V0,
                                    daemon: daemon_peer(),
                                    capabilities: BTreeSet::new(),
                                    error: Some(error),
                                },
                            )?;
                            return Ok(());
                        }
                    }
                }
                ClientEnvelope::Request {
                    request_id, method, ..
                } => {
                    if !handshaken {
                        write_message(
                            &mut writer,
                            &error_response(
                                request_id,
                                ErrorCode::NotHandshaken,
                                "handshake must complete before requests",
                                false,
                            ),
                        )?;
                        continue;
                    }

                    let response = match method.as_str() {
                        method::STATE_SNAPSHOT => success_response(
                            request_id,
                            serde_json::to_value(self.snapshot()).map_err(io::Error::other)?,
                        ),
                        method::CONNECTION_RESUME => success_response(
                            request_id,
                            json!({
                                "resumed": false,
                                "requires_snapshot": true
                            }),
                        ),
                        _ => error_response(
                            request_id,
                            ErrorCode::InvalidRequest,
                            format!("unknown method: {method}"),
                            false,
                        ),
                    };
                    write_message(&mut writer, &response)?;
                }
            }
        }
    }
}

fn daemon_peer() -> Peer {
    Peer {
        name: DAEMON_NAME.into(),
        version: DAEMON_VERSION.into(),
    }
}

fn success_response(request_id: String, result: serde_json::Value) -> ServerEnvelope {
    ServerEnvelope::Response {
        request_id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

fn error_response(
    request_id: impl Into<String>,
    code: ErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> ServerEnvelope {
    ServerEnvelope::Response {
        request_id: request_id.into(),
        ok: false,
        result: None,
        error: Some(IpcError::new(code, message, retryable)),
    }
}

fn write_message(writer: &mut impl Write, message: &ServerEnvelope) -> io::Result<()> {
    let frame = encode_frame(message).map_err(io::Error::other)?;
    writer.write_all(&frame)?;
    writer.flush()
}

/// Returns the daemon socket location shared by the native UI clients.
pub fn default_socket_path() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("GTTY_RUNTIME_DIR") {
        return Ok(PathBuf::from(path).join("gttyd.sock"));
    }
    if let Some(path) = env::var_os("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(path).join("gtty").join("gttyd.sock"));
    }
    if let Some(path) = env::var_os("HOME") {
        let home = PathBuf::from(path);
        #[cfg(target_os = "macos")]
        return Ok(home
            .join("Library")
            .join("Caches")
            .join("com.gtty.terminal")
            .join("gttyd.sock"));
        #[cfg(not(target_os = "macos"))]
        return Ok(home.join(".cache").join("gtty").join("gttyd.sock"));
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "GTTY_RUNTIME_DIR, XDG_RUNTIME_DIR, and HOME are unset",
    ))
}

/// Creates a private parent directory and binds a mode-0600 Unix socket.
pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket path must have a parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;

    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub fn run(path: &Path) -> io::Result<()> {
    let listener = bind_socket(path)?;
    let daemon = Daemon::default();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => daemon.serve_connection(stream)?,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
