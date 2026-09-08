use mitos_utils::common::permissions;
use mitos_utils::common::users;

// Example: A built-in command that shows current user info
pub fn builtin_whoami() -> Result<(), String> {
    let uid = nix::unistd::geteuid().as_raw();

    // Uses the exact same logic as the `mitos-utils` standalone binaries
    let user = users::get_user_by_uid(uid)
        .map(|u| u.name)
        .unwrap_or_else(|| format!("uid:{}", uid));

    println!("{}", user);
    Ok(())
}

// Example: Formatting file permissions for a built-in `ls` or `stat`
pub fn format_file_perms(path: &std::path::Path) -> String {
    let metadata = std::fs::metadata(path).unwrap();
    let mode = std::os::unix::fs::PermissionsExt::mode(&metadata.permissions());
    permissions::format_permissions(mode) // Returns "drwxr-xr-x"
}
