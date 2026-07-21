//! Long-lived Node worker.
//!
//! One worker process serves every request and keeps its built TypeScript
//! programs in memory. Rebuilding a program dominated request latency (roughly
//! 4.5s of a 5s request on a 5k-file project) and was discarded immediately
//! afterwards; reusing it takes warm requests to well under a second.
//!
//! The process is idle-reaped rather than kept forever, because a warm program
//! costs about 1.7 GB. It is respawned transparently on the next request.

use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::errors::SymbolPeekError;

const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const REAPER_INTERVAL: Duration = Duration::from_secs(30);

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    last_used: Instant,
}

impl Worker {
    fn spawn(script: &str, working_directory: &std::path::Path) -> std::io::Result<Self> {
        let node = std::env::var_os("SYMBOLPEEK_NODE").unwrap_or_else(|| "node".into());
        let mut child = Command::new(node)
            .arg("--input-type=commonjs")
            .arg("-e")
            .arg(script)
            .env("SYMBOLPEEK_WORKER_SERVE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .current_dir(working_directory)
            .spawn()?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            last_used: Instant::now(),
        })
    }

    /// Sends one request and reads one response line. An I/O error or EOF means
    /// the worker died and the caller should respawn.
    fn exchange(&mut self, payload: &[u8]) -> std::io::Result<String> {
        self.stdin.write_all(payload)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "TypeScript worker closed its output",
            ));
        }
        self.last_used = Instant::now();
        Ok(line)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn slot() -> &'static Mutex<Option<Worker>> {
    static SLOT: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[must_use]
pub fn enabled() -> bool {
    !matches!(
        std::env::var("SYMBOLPEEK_PERSISTENT_WORKER").as_deref(),
        Ok("0" | "false")
    )
}

fn idle_timeout() -> Duration {
    std::env::var("SYMBOLPEEK_WORKER_IDLE_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(DEFAULT_IDLE_TIMEOUT, Duration::from_secs)
}

/// Kills the worker once it has been idle past the timeout, releasing the
/// program's memory. Started on first use; harmless when the worker is absent.
fn ensure_reaper() {
    static REAPER: OnceLock<()> = OnceLock::new();
    REAPER.get_or_init(|| {
        std::thread::Builder::new()
            .name("symbolpeek-worker-reaper".to_owned())
            .spawn(|| loop {
                std::thread::sleep(REAPER_INTERVAL);
                let timeout = idle_timeout();
                let mut guard = match slot().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let expired = guard
                    .as_ref()
                    .is_some_and(|worker| worker.last_used.elapsed() >= timeout);
                if expired {
                    // Dropping the worker kills the process.
                    *guard = None;
                    crate::trace::worker_reaped(timeout.as_secs());
                }
            })
            .ok();
    });
}

/// Runs one request on the shared worker, spawning or respawning it as needed.
///
/// # Errors
///
/// Returns a parse error when the worker cannot be started or its response is
/// not readable.
pub fn request(
    script: &str,
    working_directory: &std::path::Path,
    payload: &[u8],
    path: &std::path::Path,
) -> Result<String, SymbolPeekError> {
    ensure_reaper();
    let mut guard = match slot().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // One retry: a worker reaped or crashed between requests is expected, and
    // the caller should not see it.
    let mut last_error = None;
    for _ in 0..2 {
        if guard.is_none() {
            match Worker::spawn(script, working_directory) {
                Ok(worker) => *guard = Some(worker),
                Err(error) => {
                    return Err(SymbolPeekError::Parse {
                        path: path.to_path_buf(),
                        message: format!("could not start Node.js TypeScript worker: {error}"),
                    })
                }
            }
        }
        let worker = guard.as_mut().ok_or_else(|| SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: "TypeScript worker was not available after startup".to_owned(),
        })?;
        match worker.exchange(payload) {
            Ok(line) => return Ok(line),
            Err(error) => {
                *guard = None;
                last_error = Some(error);
            }
        }
    }

    Err(SymbolPeekError::Parse {
        path: path.to_path_buf(),
        message: format!(
            "TypeScript worker failed: {}",
            last_error.map_or_else(|| "unknown error".to_owned(), |error| error.to_string())
        ),
    })
}

/// Drops the worker immediately, used on shutdown.
pub fn shutdown() {
    if let Ok(mut guard) = slot().lock() {
        *guard = None;
    }
}
