//! Shell-side half of the non-password ("ask") permission flow --
//! connects to whichever daemon owns the permission rulebook, prints
//! incoming requests without corrupting the current input line, and
//! sends back whatever `allow`/`deny` (`src/builtins/permission.rs`)
//! decides. See `super::protocol` and this module's parent doc comment
//! in `mod.rs` for the caveats on the wire format itself.

use super::protocol::{
    read_message, write_message, GrantScope, PermissionRequest, RiskLevel, ServiceToShell,
    ShellToService,
};
use crate::terminal::mrop;
use rustyline::ExternalPrinter;
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;

/// Where to reach the permission daemon. Overridable via
/// `$MITOS_SERVICE_SOCK` since no real daemon publishes a fixed path
/// yet; `/run/mitos/service.sock` just follows this project's own
/// convention of runtime sockets living under `/run/mitos/`.
fn socket_path() -> std::path::PathBuf {
    std::env::var_os("MITOS_SERVICE_SOCK")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/run/mitos/service.sock"))
}

pub struct PermissionClient {
    writer: Mutex<Option<UnixStream>>,
    pending: Arc<Mutex<HashMap<u64, PermissionRequest>>>,
}

impl PermissionClient {
    /// Cheap and infallible -- no socket is touched yet. Safe to build
    /// unconditionally in `Executor::new()`, including for one-shot
    /// script runs that will never call `spawn_listener`.
    pub fn new() -> Self {
        Self {
            writer: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Best-effort connect + register. Returns `None` (rather than an
    /// error the whole shell has to handle) if the daemon isn't
    /// reachable -- matches the doc's "fail closed" rule: no daemon
    /// just means no prompts, not a broken shell.
    fn connect(&self) -> Option<UnixStream> {
        let mut stream = UnixStream::connect(socket_path()).ok()?;
        let session_id = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "unknown".to_string());
        write_message(&mut stream, &ShellToService::Hello { session_id }).ok()?;
        Some(stream)
    }

    /// Connects (if possible) and spawns the background reader thread.
    /// Only called from the interactive `Session`, once a rustyline
    /// `ExternalPrinter` exists -- so a request arriving mid-keystroke
    /// prints as its own line instead of corrupting the one being
    /// typed.
    pub fn spawn_listener(&self, mut printer: impl ExternalPrinter + Send + 'static) {
        let Some(writer_half) = self.connect() else {
            return;
        };
        let Ok(reader_half) = writer_half.try_clone() else {
            return;
        };
        *self.writer.lock().unwrap() = Some(writer_half);

        let pending = Arc::clone(&self.pending);

        thread::spawn(move || {
            let mut reader = reader_half;
            loop {
                let message: ServiceToShell = match read_message(&mut reader) {
                    Ok(message) => message,
                    Err(_) => return, // daemon went away; stop listening
                };

                match message {
                    ServiceToShell::Request(request) => {
                        if matches!(request.risk, RiskLevel::Critical) {
                            // Should never happen -- this shell only
                            // ever registers for the "ask" tier. Log
                            // and drop rather than trust an untrusted
                            // peer to honor that on its own.
                            let _ = printer.print(format!(
                                "\n[permission] ignoring a CRITICAL request from {} (needs mitos-session)\n",
                                request.app_name
                            ));
                            continue;
                        }

                        let _ = printer.print(format!(
                            "\n[permission] {} wants: {} -- type `allow {}` or `deny {}`\n",
                            request.app_name, request.action, request.id, request.id
                        ));
                        let _ = mrop::send_button(
                            &format!("\u{2705} Allow {}", request.app_name),
                            &format!("allow {}", request.id),
                        );
                        let _ = mrop::send_button(
                            &format!("\u{274C} Deny {}", request.app_name),
                            &format!("deny {}", request.id),
                        );

                        pending.lock().unwrap().insert(request.id, request);
                    }

                    ServiceToShell::AlreadyResolved { id } => {
                        pending.lock().unwrap().remove(&id);
                        let _ = printer.print(format!(
                            "\n[permission] request {} was already resolved\n",
                            id
                        ));
                    }
                }
            }
        });
    }

    /// Resolves a pending request. Returns `false` if `id` isn't (or is
    /// no longer) pending, so `allow`/`deny` can report a clear error
    /// instead of silently doing nothing.
    pub fn resolve(&self, id: u64, allow: bool, scope: GrantScope) -> bool {
        if self.pending.lock().unwrap().remove(&id).is_none() {
            return false;
        }

        if let Some(stream) = self.writer.lock().unwrap().as_mut() {
            let _ = write_message(stream, &ShellToService::Decision { id, allow, scope });
        }

        true
    }

    /// Requests currently waiting on a decision (backs the `permissions`
    /// builtin).
    pub fn list_pending(&self) -> Vec<PermissionRequest> {
        self.pending.lock().unwrap().values().cloned().collect()
    }
}
