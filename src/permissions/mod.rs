//! MITOS app-permission requests: the "App X wants permission" flow
//! from the MITOS Permissions design doc. This is unrelated to Unix
//! file-mode permissions (`chmod`-style bits) -- see
//! `mitos_utils::common::permissions` / `crate::util::format_permissions`
//! for that.
//!
//! mitos-shell only ever hosts the non-password ("ask") tier: a low/
//! medium-risk request that just needs an Allow/Deny answer. Anything
//! requiring a password is exclusively mitos-session + mitos-gui's job
//! (per the doc's own rule: only the compositor draws password
//! prompts), so this module never offers a permanent ("Always allow")
//! grant and never touches a password.
//!
//! The wire protocol here (`protocol.rs`) is this shell's best-effort
//! read of the design doc, using the same "Unix socket, length-prefixed
//! bincode" transport mitos-session already uses talking to mitos-gui.
//! No permission-daemon crate/protocol was available to build against
//! directly, so treat these message types as a placeholder to reconcile
//! once that daemon (or whichever real component ends up owning this)
//! exposes its own shared types.

pub mod client;
pub mod protocol;
