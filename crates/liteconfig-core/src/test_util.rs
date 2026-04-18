//! Shared test utilities. All tests that touch `LITECONFIG_HOME` must go
//! through `with_temp_home()` so they serialize on the same mutex — otherwise
//! parallel tests stomp each other's env var.

#![cfg(test)]

use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct TempHome {
    #[allow(dead_code)]
    pub dir: TempDir,
    _guard: MutexGuard<'static, ()>,
}

pub fn with_temp_home() -> TempHome {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    TempHome { dir, _guard: guard }
}
