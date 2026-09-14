//! Wire messages between mitos-shell and whichever daemon owns the
//! non-password permission "rulebook" (the MITOS Permissions doc calls
//! it mitos-service; no such crate exists yet among this project's real
//! repos as of this writing, so this is written directly from the doc's
//! own request/response narrative and its "Unix sockets, bincode"
//! glossary entry -- kept deliberately small and self-contained so it's
//! a quick swap for that daemon's real shared types later).
//!
//! Framing: each message is a little-endian u32 byte length, then that
//! many bytes of bincode-encoded payload. Plain length-prefixing (on
//! top of bincode's own encoding) is what lets a stream socket tell
//! where one message ends and the next begins.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// Matches the MITOS Permissions doc's two-tier risk model. mitos-shell
/// should only ever be handed `Ask` requests -- `Critical` ones are
/// mitos-session + mitos-gui's job -- but the type carries both so a
/// mis-routed `Critical` request can be detected and rejected instead
/// of silently trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Ask,
    Critical,
}

/// A pending request forwarded for a non-password decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: u64,
    pub app_name: String,
    pub action: String,
    pub risk: RiskLevel,
}

/// How long an `allow` should last. Deliberately has no `Always` --
/// the doc requires a password for permanent grants, which this shell
/// never collects.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GrantScope {
    Once,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellToService {
    /// Registers this shell instance so the daemon knows where to route
    /// non-password requests for this session.
    Hello { session_id: String },
    Decision {
        id: u64,
        allow: bool,
        scope: GrantScope,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceToShell {
    Request(PermissionRequest),
    /// The daemon's answer when a decision comes back for an id that
    /// already expired or was resolved elsewhere (e.g. mitos-settings).
    AlreadyResolved {
        id: u64,
    },
}

pub fn write_message<T: Serialize>(stream: &mut impl Write, message: &T) -> io::Result<()> {
    let bytes = bincode::serialize(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}

pub fn read_message<T: DeserializeOwned>(stream: &mut impl Read) -> io::Result<T> {
    let mut length_bytes = [0u8; 4];
    stream.read_exact(&mut length_bytes)?;
    let mut payload = vec![0u8; u32::from_le_bytes(length_bytes) as usize];
    stream.read_exact(&mut payload)?;
    bincode::deserialize(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
