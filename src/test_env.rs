#[cfg(test)]
pub fn set_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, val: V) {
    // Tests using this helper must run serially when mutating process-wide env.
    unsafe { std::env::set_var(key, val) };
}

#[cfg(test)]
pub fn remove_var<K: AsRef<std::ffi::OsStr>>(key: K) {
    // Tests using this helper must run serially when mutating process-wide env.
    unsafe { std::env::remove_var(key) };
}
