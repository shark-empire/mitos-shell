#[inline]
pub fn set_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    #[allow(unused_unsafe)]
    unsafe { std::env::set_var(key, value); }
}
