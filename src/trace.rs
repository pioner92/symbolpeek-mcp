//! Opt-in server-side request timing.
//!
//! Enabled with `SYMBOLPEEK_TRACE=1`; every line goes to stderr, which MCP
//! clients capture in their server logs. The point is to separate time the
//! server actually spends from time the client adds around the call, so a
//! reported "30 seconds" can be attributed instead of guessed at.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static IN_FLIGHT: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var_os("SYMBOLPEEK_TRACE").is_some_and(|value| value != "0" && value != "false")
    })
}

fn uptime_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

/// One traced tool call. Phases are recorded as they complete; the summary is
/// emitted on drop so an early return still produces a line.
pub struct RequestTrace {
    tool: String,
    sequence: u64,
    started: Instant,
    started_at_ms: u128,
}

impl RequestTrace {
    #[must_use]
    pub fn start(tool: &str) -> Option<Self> {
        if !enabled() {
            return None;
        }
        let tool = tool.to_owned();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let concurrent = IN_FLIGHT.fetch_add(1, Ordering::Relaxed) + 1;
        let started_at_ms = uptime_ms();
        eprintln!(
            "[symbolpeek] #{sequence} {tool} start t={started_at_ms}ms in_flight={concurrent}"
        );
        Some(Self {
            tool,
            sequence,
            started: Instant::now(),
            started_at_ms,
        })
    }
}

impl Drop for RequestTrace {
    fn drop(&mut self) {
        let total = self.started.elapsed().as_millis();
        let concurrent = IN_FLIGHT.fetch_sub(1, Ordering::Relaxed) - 1;
        eprintln!(
            "[symbolpeek] #{} {} done t={}ms..{}ms total={total}ms still_in_flight={concurrent}",
            self.sequence,
            self.tool,
            self.started_at_ms,
            self.started_at_ms + total,
        );
    }
}

/// Traces one Node worker invocation, reported as a nested line so worker time
/// can be compared against the enclosing tool call.
pub fn worker(operation: &str, elapsed_ms: u128, program_files: Option<usize>) {
    if !enabled() {
        return;
    }
    match program_files {
        Some(files) => {
            eprintln!("[symbolpeek]   worker {operation} {elapsed_ms}ms program_files={files}");
        }
        None => eprintln!("[symbolpeek]   worker {operation} {elapsed_ms}ms"),
    }
}

/// Reports that the idle worker was killed and its programs released.
pub fn worker_reaped(idle_secs: u64) {
    if enabled() {
        eprintln!("[symbolpeek] worker reaped after {idle_secs}s idle");
    }
}
