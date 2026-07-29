//! Minimal GTTY daemon transport and IPC v0 handshake.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gtty_protocol::{
    ClientEnvelope, DaemonSnapshot, ErrorCode, EventPayload, IPC_V0, IpcError, MAX_FRAME_BYTES,
    Peer, ServerEnvelope, capability, decode_frame, encode_frame, method, negotiate,
};
use serde_json::json;

pub const DAEMON_NAME: &str = "gttyd";
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_CONNECTIONS: usize = 32;
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

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
        return Ok(PathBuf::from(path).join("gtty").join("gttyd.sock"));
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
    if !parent.try_exists()? {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }

    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket parent must be a real directory",
        ));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "socket parent must not be accessible by group or other users",
        ));
    }

    let listener = UnixListener::bind(path)?;
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(listener)
}

pub fn run(path: &Path) -> io::Result<()> {
    let listener = bind_socket(path)?;
    let daemon = Arc::new(Daemon::default());
    let active_connections = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        let Some(permit) = ConnectionPermit::acquire(&active_connections) else {
            log_connection_error(
                "connection.rejected",
                connection_id,
                &io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!("maximum of {MAX_CONNECTIONS} clients already connected"),
                ),
            );
            continue;
        };

        let daemon = Arc::clone(&daemon);
        let spawn_result = thread::Builder::new()
            .name(format!("gttyd-client-{connection_id}"))
            .spawn(move || {
                let _permit = permit;
                if let Err(error) = daemon.serve_connection(stream) {
                    log_connection_error("connection.failed", connection_id, &error);
                }
            });
        match spawn_result {
            Ok(handle) => drop(handle),
            Err(error) => {
                log_connection_error("connection.spawn_failed", connection_id, &error);
            }
        }
    }
    Ok(())
}

struct ConnectionPermit {
    active_connections: Arc<AtomicUsize>,
}

impl ConnectionPermit {
    fn acquire(active_connections: &Arc<AtomicUsize>) -> Option<Self> {
        active_connections
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_CONNECTIONS).then_some(current + 1)
            })
            .ok()
            .map(|_| Self {
                active_connections: Arc::clone(active_connections),
            })
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.active_connections.fetch_sub(1, Ordering::AcqRel);
    }
}

fn log_connection_error(event: &str, connection_id: u64, error: &io::Error) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        });
    eprintln!(
        "{}",
        json!({
            "timestamp": timestamp,
            "level": "error",
            "component": DAEMON_NAME,
            "event": event,
            "connection_id": connection_id,
            "request_id": null,
            "task_id": null,
            "session_id": null,
            "error_code": "io",
            "retryable": true,
            "duration_ms": null,
            "message": error.to_string(),
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_permits_enforce_and_release_the_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let permits = (0..MAX_CONNECTIONS)
            .map(|_| ConnectionPermit::acquire(&active).expect("permit must be available"))
            .collect::<Vec<_>>();

        assert!(ConnectionPermit::acquire(&active).is_none());
        drop(permits);
        assert!(ConnectionPermit::acquire(&active).is_some());
    }
}
