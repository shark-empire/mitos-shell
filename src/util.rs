#[inline]
pub fn set_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Returns true if `name` resolves to an executable file: a path
/// containing `/` is checked directly, otherwise every directory on
/// `$PATH` is searched. Used to fail fast (and report
/// [`crate::error::ShellError::NotFound`]) before forking, rather than
/// spawning a child process just to watch it fail.
pub fn command_exists(name: &str) -> bool {
    if name.contains('/') {
        return std::path::Path::new(name).is_file();
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}
