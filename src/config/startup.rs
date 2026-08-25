use std::path::PathBuf;

/// Returns the startup files loaded by MITOS shell.
///
/// Order:
///   1. /etc/mitosrc
///   2. ~/.mitosrc
pub fn files() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // System-wide configuration.
    paths.push(PathBuf::from("/etc/mitosrc"));

    // User configuration.
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".mitosrc"));
    }

    paths
}
