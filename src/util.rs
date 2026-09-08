use mitos_utils::common::{paths, permissions, users};
use std::os::unix::fs::PermissionsExt;

// ────────────────────────── Environment Variables ──────────────────────────

#[inline]
pub fn set_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var(key, value);
    }
}

// ────────────────────────── Command Resolution ──────────────────────────

/// Returns true if `name` resolves to an executable file: a path
/// containing `/` is checked directly, otherwise every directory on
/// `$PATH` is searched. Used to fail fast (and report
/// [`crate::error::ShellError::NotFound`]) before forking, rather than
/// spawning a child process just to watch it fail.
pub fn command_exists(name: &str) -> bool {
    if name.contains('/') {
        let path = std::path::Path::new(name);
        return path.is_file() && is_executable(path);
    }

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let full_path = dir.join(name);
                full_path.is_file() && is_executable(&full_path)
            })
        })
        .unwrap_or(false)
}

/// Helper to check if a file has the executable bit set for the current user.
/// This prevents the shell from attempting to execute plain text files
/// that happen to be on the $PATH.
#[inline]
fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| (m.permissions().mode() & 0o111) != 0)
        .unwrap_or(false)
}

// ────────────────────────── MITOS-UTILS INTEGRATIONS ──────────────────────────
//
// These wrappers expose the `mitos-utils` common library to the rest of
// the shell. By routing all path/user/permission logic through here, we
// guarantee that the shell's built-in behaviors (like `cd ~` or `ls -l`)
// match the standalone MITOS coreutils exactly.

/// Expands a leading `~` to the user's home directory.
/// Delegates to mitos-utils to ensure identical behavior to `mitos-cd` and `mitos-ls`.
pub fn expand_tilde(path: &str) -> std::path::PathBuf {
    paths::expand_tilde(path) // Now works because we added it to mitos-utils!
}

/// Safely resolves a path, normalizing `.` and `..` without resolving symlinks.
pub fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    // mitos-utils requires a `cwd` argument to resolve relative paths lexically
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
    paths::normalize(path, &cwd)
}

/// Formats Unix permissions (e.g., "drwxr-xr-x") using the exact same
/// bitmask logic as the standalone `mitos-ls` applet.
pub fn format_permissions(mode: u32) -> String {
    permissions::format_permissions(mode) // Now works because we added it to mitos-utils!
}

/// Gets the username for a given UID.
/// Useful for prompt customization (e.g., `user@mitos:~$`).
pub fn get_username_by_uid(uid: u32) -> Option<String> {
    // mitos-utils uses `name_for_uid`, not `get_user_by_uid`
    users::name_for_uid(uid)
}

/// Gets the current user's username for the shell prompt.
pub fn get_current_username() -> String {
    let uid = nix::unistd::geteuid().as_raw();
    get_username_by_uid(uid).unwrap_or_else(|| "unknown".to_string())
}
