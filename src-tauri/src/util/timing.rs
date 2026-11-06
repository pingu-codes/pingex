//! Startup phase timing, off unless `PINGEX_TRACE_STARTUP=1`.
//!
//! Each phase prints `[startup] <name> <ms> (+<since launch>ms)` to stderr when
//! it ends, so `scripts/startup-timing.sh` can rank where the launch goes. It
//! is the startup counterpart of `PINGEX_TRACE_IO` in `util::process`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

fn launch() -> Instant {
    static LAUNCH: OnceLock<Instant> = OnceLock::new();
    *LAUNCH.get_or_init(Instant::now)
}

pub(crate) fn enabled() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE
        .get_or_init(|| std::env::var_os("PINGEX_TRACE_STARTUP").is_some_and(|value| value == "1"))
}

/// Pin the launch instant; called first thing in `run`.
pub(crate) fn start() {
    if enabled() {
        launch();
    }
}

/// A phase that reports its duration when dropped.
pub(crate) struct Phase {
    name: Option<String>,
    started: Instant,
}

impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(name) = self.name.take() {
            eprintln!(
                "[startup] {name} {}ms (+{}ms)",
                self.started.elapsed().as_millis(),
                launch().elapsed().as_millis()
            );
        }
    }
}

pub(crate) fn phase(name: impl Into<String>) -> Phase {
    Phase {
        name: enabled().then(|| name.into()),
        started: Instant::now(),
    }
}

/// A point in time rather than a span (`bootstrap_profile done`).
pub(crate) fn mark(name: &str) {
    if enabled() {
        eprintln!("[startup] {name} (+{}ms)", launch().elapsed().as_millis());
    }
}

static LOCK_WAITS: AtomicU64 = AtomicU64::new(0);
static LOCK_WAIT_MS: AtomicU64 = AtomicU64::new(0);

/// Record a statement that waited behind another connection's write.
pub(crate) fn lock_wait(tries: u32, waited: std::time::Duration) {
    if enabled() && tries > 0 {
        LOCK_WAITS.fetch_add(u64::from(tries), Ordering::Relaxed);
        LOCK_WAIT_MS.fetch_add(waited.as_millis() as u64, Ordering::Relaxed);
    }
}

/// Print the lock-wait totals so far.
pub(crate) fn report_lock_waits(label: &str) {
    if enabled() {
        eprintln!(
            "[startup] db lock retries at {label}: {} ({}ms waiting)",
            LOCK_WAITS.load(Ordering::Relaxed),
            LOCK_WAIT_MS.load(Ordering::Relaxed)
        );
    }
}
