//! `allow` / `deny` / `permissions` builtins: the user's side of the
//! non-password permission flow described in the MITOS Permissions doc.
//! See `crate::permissions` for the client these call into.

use crate::execution::executor::Executor;
use crate::permissions::protocol::GrantScope;

/// `allow <id> [once|session]` -- approves a pending request. No
/// "always" option: a permanent grant needs a password under the MITOS
/// permission model, which is mitos-session + mitos-gui's job, not
/// this shell's.
pub fn allow(executor: &Executor, args: &[String]) -> i32 {
    let Some(id) = args.get(1).and_then(|value| value.parse::<u64>().ok()) else {
        eprintln!("allow: usage: allow <id> [once|session]");
        return 1;
    };

    let scope = match args.get(2).map(String::as_str) {
        None | Some("once") => GrantScope::Once,
        Some("session") => GrantScope::Session,
        Some(other) => {
            eprintln!("allow: unknown scope '{}' (expected once|session)", other);
            return 1;
        }
    };

    if executor.permissions.resolve(id, true, scope) {
        0
    } else {
        eprintln!("allow: no pending permission request with id {}", id);
        1
    }
}

/// `deny <id>` -- declines a pending request.
pub fn deny(executor: &Executor, args: &[String]) -> i32 {
    let Some(id) = args.get(1).and_then(|value| value.parse::<u64>().ok()) else {
        eprintln!("deny: usage: deny <id>");
        return 1;
    };

    if executor.permissions.resolve(id, false, GrantScope::Once) {
        0
    } else {
        eprintln!("deny: no pending permission request with id {}", id);
        1
    }
}

/// `permissions` -- lists requests currently waiting on a decision.
pub fn list(executor: &Executor) -> i32 {
    let pending = executor.permissions.list_pending();

    if pending.is_empty() {
        println!("No pending permission requests.");
    } else {
        for request in pending {
            println!(
                "[{}] {} wants: {}",
                request.id, request.app_name, request.action
            );
        }
    }

    0
}
