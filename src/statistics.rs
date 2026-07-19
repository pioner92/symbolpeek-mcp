//! Lightweight context-avoidance statistics for current and lifetime sessions.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::types::SymbolContextResult;

const BYTES_PER_ESTIMATED_TOKEN: i64 = 4;

/// Source measurements used by the statistics collectors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceMetrics {
    pub bytes: u64,
    pub lines: u64,
}

impl SourceMetrics {
    /// Measures source without allocating or parsing it.
    #[must_use]
    pub fn from_source(source: &str) -> Self {
        Self {
            bytes: u64::try_from(source.len()).unwrap_or(u64::MAX),
            lines: u64::try_from(source.lines().count()).unwrap_or(u64::MAX),
        }
    }

    /// Adds one returned source fragment to the aggregate response size.
    pub fn add_source(&mut self, source: &str) {
        let metrics = Self::from_source(source);
        self.bytes = self.bytes.saturating_add(metrics.bytes);
        self.lines = self.lines.saturating_add(metrics.lines);
    }

    /// Measures all source fragments returned by `read_symbol_context`.
    #[must_use]
    pub fn from_context(context: &SymbolContextResult) -> Self {
        let mut metrics = Self::default();
        metrics.add_source(&context.requested_symbol.source);
        for symbol in &context.helper_functions {
            metrics.add_source(&symbol.source);
        }
        for symbol in &context.local_types {
            metrics.add_source(&symbol.source);
        }
        for symbol in &context.local_constants {
            metrics.add_source(&symbol.source);
        }
        metrics
    }
}

/// JSON/MCP representation of one statistics scope.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StatisticsSnapshot {
    pub successful_requests: u64,
    pub files_avoided: u64,
    pub lines_avoided: i64,
    pub bytes_avoided: i64,
    /// Approximation using four source bytes per token; never model-specific.
    pub estimated_token_savings: i64,
    /// Percentage, not a fraction. This value is explicitly an estimate.
    pub average_context_reduction_percent: f64,
}

/// Statistics returned by the MCP `get_statistics` tool.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StatisticsReport {
    pub session: StatisticsSnapshot,
    pub lifetime: StatisticsSnapshot,
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    successful_requests: u64,
    files_avoided: u64,
    lines_avoided: i64,
    bytes_avoided: i64,
    total_reduction_percent: f64,
}

/// Session-only statistics. It deliberately has no persistence or background work.
#[derive(Debug, Default)]
pub struct SessionStatistics {
    accumulator: Mutex<SessionAccumulator>,
}

impl SessionStatistics {
    /// Records one successful semantic request.
    pub fn record(&self, original: SourceMetrics, returned: SourceMetrics) {
        let mut accumulator = self
            .accumulator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        accumulator.successful_requests = accumulator.successful_requests.saturating_add(1);
        accumulator.files_avoided = accumulator.files_avoided.saturating_add(1);
        accumulator.lines_avoided = accumulator
            .lines_avoided
            .saturating_add(signed_difference(original.lines, returned.lines));
        accumulator.bytes_avoided = accumulator
            .bytes_avoided
            .saturating_add(signed_difference(original.bytes, returned.bytes));
        accumulator.total_reduction_percent += reduction_percent(original.bytes, returned.bytes);
    }

    /// Returns a consistent point-in-time snapshot.
    #[must_use]
    pub fn snapshot(&self) -> StatisticsSnapshot {
        let accumulator = self
            .accumulator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        StatisticsSnapshot {
            successful_requests: accumulator.successful_requests,
            files_avoided: accumulator.files_avoided,
            lines_avoided: accumulator.lines_avoided,
            bytes_avoided: accumulator.bytes_avoided,
            estimated_token_savings: accumulator.bytes_avoided / BYTES_PER_ESTIMATED_TOKEN,
            average_context_reduction_percent: average_reduction(
                accumulator.total_reduction_percent,
                accumulator.successful_requests,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct LifetimeAccumulator {
    files_avoided: u64,
    lines_avoided: i64,
    bytes_avoided: i64,
    estimated_tokens_avoided: i64,
    average_reduction_percent: f64,
}

impl LifetimeAccumulator {
    #[allow(clippy::cast_precision_loss)]
    fn record(&mut self, original: SourceMetrics, returned: SourceMetrics) {
        let reduction = reduction_percent(original.bytes, returned.bytes);
        let previous_requests = self.files_avoided;
        self.files_avoided = self.files_avoided.saturating_add(1);
        self.lines_avoided = self
            .lines_avoided
            .saturating_add(signed_difference(original.lines, returned.lines));
        self.bytes_avoided = self
            .bytes_avoided
            .saturating_add(signed_difference(original.bytes, returned.bytes));
        self.estimated_tokens_avoided = self.estimated_tokens_avoided.saturating_add(
            signed_difference(original.bytes, returned.bytes) / BYTES_PER_ESTIMATED_TOKEN,
        );
        self.average_reduction_percent = if self.files_avoided == 0 {
            0.0
        } else {
            ((self.average_reduction_percent * previous_requests as f64) + reduction)
                / self.files_avoided as f64
        };
    }

    fn snapshot(self) -> StatisticsSnapshot {
        StatisticsSnapshot {
            successful_requests: self.files_avoided,
            files_avoided: self.files_avoided,
            lines_avoided: self.lines_avoided,
            bytes_avoided: self.bytes_avoided,
            estimated_token_savings: self.estimated_tokens_avoided,
            average_context_reduction_percent: self.average_reduction_percent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLifetimeStatistics {
    #[serde(rename = "filesAvoided")]
    files_avoided: u64,
    #[serde(rename = "linesAvoided")]
    lines_avoided: i64,
    #[serde(rename = "bytesAvoided")]
    bytes_avoided: i64,
    #[serde(rename = "estimatedTokensAvoided")]
    estimated_tokens_avoided: i64,
    #[serde(rename = "averageReduction")]
    average_reduction: f64,
}

impl From<LifetimeAccumulator> for PersistedLifetimeStatistics {
    fn from(accumulator: LifetimeAccumulator) -> Self {
        Self {
            files_avoided: accumulator.files_avoided,
            lines_avoided: accumulator.lines_avoided,
            bytes_avoided: accumulator.bytes_avoided,
            estimated_tokens_avoided: accumulator.estimated_tokens_avoided,
            average_reduction: accumulator.average_reduction_percent,
        }
    }
}

impl From<PersistedLifetimeStatistics> for LifetimeAccumulator {
    fn from(persisted: PersistedLifetimeStatistics) -> Self {
        Self {
            files_avoided: persisted.files_avoided,
            lines_avoided: persisted.lines_avoided,
            bytes_avoided: persisted.bytes_avoided,
            estimated_tokens_avoided: persisted.estimated_tokens_avoided,
            average_reduction_percent: if persisted.average_reduction.is_finite() {
                persisted.average_reduction
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug)]
struct PersistenceState {
    path: Option<PathBuf>,
    enabled: bool,
    last_persisted_request_count: u64,
}

/// Lifetime statistics backed by one human-readable JSON file.
///
/// All persistence failures are intentionally swallowed and disable persistence
/// for this instance. The in-memory counters continue to work normally.
#[derive(Debug)]
pub struct LifetimeStatistics {
    accumulator: Mutex<LifetimeAccumulator>,
    persistence: Mutex<PersistenceState>,
}

impl LifetimeStatistics {
    /// Loads lifetime statistics from the platform-appropriate user directory.
    #[must_use]
    pub fn load_default() -> Self {
        Self::from_optional_path(default_statistics_path())
    }

    /// Creates a collector using an explicit path, primarily for isolated tests.
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        Self::from_optional_path(Some(path))
    }

    fn from_optional_path(path: Option<PathBuf>) -> Self {
        let (accumulator, enabled) = match path.as_deref() {
            Some(path) => load_from_path(path),
            None => (LifetimeAccumulator::default(), false),
        };
        let last_persisted_request_count = accumulator.files_avoided;
        Self {
            accumulator: Mutex::new(accumulator),
            persistence: Mutex::new(PersistenceState {
                path,
                enabled,
                last_persisted_request_count,
            }),
        }
    }

    /// Records and persists one successful semantic request.
    pub fn record(&self, original: SourceMetrics, returned: SourceMetrics) {
        let (snapshot, request_count) = {
            let mut accumulator = self
                .accumulator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            accumulator.record(original, returned);
            (accumulator.snapshot(), accumulator.files_avoided)
        };
        self.persist(snapshot, request_count, false);
    }

    /// Returns a consistent point-in-time lifetime snapshot.
    #[must_use]
    pub fn snapshot(&self) -> StatisticsSnapshot {
        self.accumulator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
    }

    /// Resets lifetime counters and persists the zeroed aggregate when possible.
    pub fn reset(&self) {
        let snapshot = {
            let mut accumulator = self
                .accumulator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *accumulator = LifetimeAccumulator::default();
            accumulator.snapshot()
        };
        self.persist(snapshot, 0, true);
    }

    fn persist(&self, snapshot: StatisticsSnapshot, request_count: u64, force: bool) {
        let mut persistence = self
            .persistence
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !persistence.enabled
            || (!force && request_count <= persistence.last_persisted_request_count)
        {
            return;
        }
        let Some(path) = persistence.path.as_deref() else {
            persistence.enabled = false;
            return;
        };
        let Some(parent) = path.parent() else {
            persistence.enabled = false;
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            persistence.enabled = false;
            return;
        }
        let persisted = PersistedLifetimeStatistics {
            files_avoided: snapshot.files_avoided,
            lines_avoided: snapshot.lines_avoided,
            bytes_avoided: snapshot.bytes_avoided,
            estimated_tokens_avoided: snapshot.estimated_token_savings,
            average_reduction: snapshot.average_context_reduction_percent,
        };
        let Ok(contents) = serde_json::to_string_pretty(&persisted) else {
            persistence.enabled = false;
            return;
        };
        let temporary_path = path.with_extension("json.tmp");
        if fs::write(&temporary_path, format!("{contents}\n")).is_err()
            || fs::rename(&temporary_path, path).is_err()
        {
            persistence.enabled = false;
            return;
        }
        persistence.last_persisted_request_count = request_count;
    }
}

fn load_from_path(path: &Path) -> (LifetimeAccumulator, bool) {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<PersistedLifetimeStatistics>(&contents) {
            Ok(persisted) => (persisted.into(), true),
            Err(_) => (LifetimeAccumulator::default(), false),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => (LifetimeAccumulator::default(), true),
        Err(_) => (LifetimeAccumulator::default(), false),
    }
}

fn default_statistics_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SYMBOLPEEK_STATS_PATH") {
        return Some(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/SymbolPeek/stats.json"))
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|app_data| app_data.join("SymbolPeek/stats.json"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|config| config.join("symbolpeek/stats.json"))
    }
}

fn signed_difference(original: u64, returned: u64) -> i64 {
    let original = i64::try_from(original).unwrap_or(i64::MAX);
    let returned = i64::try_from(returned).unwrap_or(i64::MAX);
    original.saturating_sub(returned)
}

#[allow(clippy::cast_precision_loss)]
fn reduction_percent(original_bytes: u64, returned_bytes: u64) -> f64 {
    if original_bytes == 0 {
        return 0.0;
    }
    let original_bytes = original_bytes as f64;
    let returned_bytes = returned_bytes as f64;
    ((original_bytes - returned_bytes) / original_bytes) * 100.0
}

#[allow(clippy::cast_precision_loss)]
fn average_reduction(total_reduction_percent: f64, request_count: u64) -> f64 {
    if request_count == 0 {
        0.0
    } else {
        total_reduction_percent / request_count as f64
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{LifetimeStatistics, SessionStatistics, SourceMetrics};

    static NEXT_TEST_PATH: AtomicU64 = AtomicU64::new(0);

    fn temporary_statistics_path() -> PathBuf {
        let sequence = NEXT_TEST_PATH.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "symbolpeek-statistics-{}-{sequence}-{timestamp}.json",
            std::process::id()
        ))
    }

    #[test]
    fn records_session_aggregate_avoidance_and_average_reduction() {
        let statistics = SessionStatistics::default();
        statistics.record(
            SourceMetrics {
                bytes: 100,
                lines: 20,
            },
            SourceMetrics {
                bytes: 20,
                lines: 5,
            },
        );
        statistics.record(
            SourceMetrics {
                bytes: 200,
                lines: 40,
            },
            SourceMetrics {
                bytes: 100,
                lines: 20,
            },
        );

        assert_eq!(statistics.snapshot().files_avoided, 2);
        assert_eq!(statistics.snapshot().lines_avoided, 35);
        assert_eq!(statistics.snapshot().bytes_avoided, 180);
        assert_eq!(statistics.snapshot().estimated_token_savings, 45);
        assert!(
            (statistics.snapshot().average_context_reduction_percent - 65.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn measures_utf8_bytes_and_source_lines() {
        assert_eq!(
            SourceMetrics::from_source("é\nconst value = 1;"),
            SourceMetrics {
                bytes: 19,
                lines: 2
            }
        );
        assert_eq!(SourceMetrics::from_source(""), SourceMetrics::default());
    }

    #[test]
    fn lifetime_statistics_survive_reload_and_reset() {
        let path = temporary_statistics_path();
        let statistics = LifetimeStatistics::from_path(path.clone());
        statistics.record(
            SourceMetrics {
                bytes: 100,
                lines: 20,
            },
            SourceMetrics {
                bytes: 20,
                lines: 5,
            },
        );
        let persisted = fs::read_to_string(&path).expect("statistics should be persisted");
        assert!(persisted.contains("filesAvoided"));
        assert!(persisted.contains("estimatedTokensAvoided"));

        let loaded = LifetimeStatistics::from_path(path.clone());
        assert_eq!(loaded.snapshot().files_avoided, 1);
        assert_eq!(loaded.snapshot().bytes_avoided, 80);
        assert_eq!(loaded.snapshot().estimated_token_savings, 20);

        loaded.reset();
        let reset = LifetimeStatistics::from_path(path.clone());
        assert_eq!(reset.snapshot().files_avoided, 0);
        assert_eq!(reset.snapshot().bytes_avoided, 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_persistence_disables_writes_without_affecting_memory() {
        let path = temporary_statistics_path();
        fs::write(&path, "not-json").expect("test statistics file should be writable");
        let statistics = LifetimeStatistics::from_path(path.clone());
        statistics.record(
            SourceMetrics {
                bytes: 100,
                lines: 20,
            },
            SourceMetrics {
                bytes: 20,
                lines: 5,
            },
        );
        assert_eq!(statistics.snapshot().files_avoided, 1);
        assert_eq!(
            fs::read_to_string(&path).expect("file should remain readable"),
            "not-json"
        );
        let _ = fs::remove_file(path);
    }
}
