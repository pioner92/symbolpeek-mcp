//! Lightweight context-avoidance statistics for current and lifetime sessions.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

const BYTES_PER_ESTIMATED_TOKEN: i64 = 4;

/// Human-readable explanation of how the reported numbers are derived.
pub const STATISTICS_NOTE: &str =
    "Directional estimate. Baseline = full source bytes/lines of distinct files represented by \
each successful result; returned size = compact serialized semantic data with singular file-path \
fields excluded; token savings assume ~4 bytes per token.";

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

    /// Adds one source fragment to the aggregate size.
    pub fn add_source(&mut self, source: &str) {
        let metrics = Self::from_source(source);
        self.bytes = self.bytes.saturating_add(metrics.bytes);
        self.lines = self.lines.saturating_add(metrics.lines);
    }
}

/// One recorded request: a full-source counterfactual versus the compact
/// semantic-result estimate.
#[derive(Debug, Clone, Copy)]
pub struct RequestSample {
    /// Aggregate size of the distinct source files represented by the result.
    pub original: SourceMetrics,
    /// Size of the compact serialized semantic result used by the estimate.
    pub returned: SourceMetrics,
    /// Number of distinct files represented by the result under the
    /// full-source baseline.
    pub files: u64,
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
    /// Size-weighted percentage across all requests. Explicitly an estimate.
    pub average_context_reduction_percent: f64,
}

/// Statistics returned by the MCP `get_statistics` tool.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StatisticsReport {
    pub session: StatisticsSnapshot,
    pub lifetime: StatisticsSnapshot,
    /// How to read the numbers above.
    pub note: &'static str,
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    successful_requests: u64,
    files_avoided: u64,
    lines_avoided: i64,
    bytes_avoided: i64,
    total_original_bytes: i64,
}

/// Session-only statistics. It deliberately has no persistence or background work.
#[derive(Debug, Default)]
pub struct SessionStatistics {
    accumulator: Mutex<SessionAccumulator>,
}

impl SessionStatistics {
    /// Records one successful semantic request.
    pub fn record(&self, sample: RequestSample) {
        let mut accumulator = self
            .accumulator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        accumulator.successful_requests = accumulator.successful_requests.saturating_add(1);
        accumulator.files_avoided = accumulator.files_avoided.saturating_add(sample.files);
        accumulator.lines_avoided = accumulator.lines_avoided.saturating_add(signed_difference(
            sample.original.lines,
            sample.returned.lines,
        ));
        accumulator.bytes_avoided = accumulator.bytes_avoided.saturating_add(signed_difference(
            sample.original.bytes,
            sample.returned.bytes,
        ));
        accumulator.total_original_bytes = accumulator
            .total_original_bytes
            .saturating_add(as_i64(sample.original.bytes));
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
            average_context_reduction_percent: weighted_reduction(
                accumulator.bytes_avoided,
                accumulator.total_original_bytes,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct LifetimeAccumulator {
    successful_requests: u64,
    files_avoided: u64,
    lines_avoided: i64,
    bytes_avoided: i64,
    total_original_bytes: i64,
}

impl LifetimeAccumulator {
    fn record(&mut self, sample: RequestSample) {
        self.successful_requests = self.successful_requests.saturating_add(1);
        self.files_avoided = self.files_avoided.saturating_add(sample.files);
        self.lines_avoided = self.lines_avoided.saturating_add(signed_difference(
            sample.original.lines,
            sample.returned.lines,
        ));
        self.bytes_avoided = self.bytes_avoided.saturating_add(signed_difference(
            sample.original.bytes,
            sample.returned.bytes,
        ));
        self.total_original_bytes = self
            .total_original_bytes
            .saturating_add(as_i64(sample.original.bytes));
    }

    fn snapshot(self) -> StatisticsSnapshot {
        StatisticsSnapshot {
            successful_requests: self.successful_requests,
            files_avoided: self.files_avoided,
            lines_avoided: self.lines_avoided,
            bytes_avoided: self.bytes_avoided,
            estimated_token_savings: self.bytes_avoided / BYTES_PER_ESTIMATED_TOKEN,
            average_context_reduction_percent: weighted_reduction(
                self.bytes_avoided,
                self.total_original_bytes,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLifetimeStatistics {
    #[serde(rename = "successfulRequests", default)]
    successful_requests: u64,
    #[serde(rename = "filesAvoided")]
    files_avoided: u64,
    #[serde(rename = "linesAvoided")]
    lines_avoided: i64,
    #[serde(rename = "bytesAvoided")]
    bytes_avoided: i64,
    #[serde(rename = "estimatedTokensAvoided")]
    estimated_tokens_avoided: i64,
    #[serde(rename = "totalOriginalBytes", default)]
    total_original_bytes: i64,
    #[serde(rename = "averageReduction")]
    average_reduction: f64,
}

impl From<LifetimeAccumulator> for PersistedLifetimeStatistics {
    fn from(accumulator: LifetimeAccumulator) -> Self {
        Self {
            successful_requests: accumulator.successful_requests,
            files_avoided: accumulator.files_avoided,
            lines_avoided: accumulator.lines_avoided,
            bytes_avoided: accumulator.bytes_avoided,
            estimated_tokens_avoided: accumulator.bytes_avoided / BYTES_PER_ESTIMATED_TOKEN,
            total_original_bytes: accumulator.total_original_bytes,
            average_reduction: weighted_reduction(
                accumulator.bytes_avoided,
                accumulator.total_original_bytes,
            ),
        }
    }
}

impl From<PersistedLifetimeStatistics> for LifetimeAccumulator {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn from(persisted: PersistedLifetimeStatistics) -> Self {
        // Legacy files stored only an average percentage; reconstruct a
        // consistent byte baseline so the weighted average stays continuous.
        let total_original_bytes = if persisted.total_original_bytes > 0 {
            persisted.total_original_bytes
        } else if persisted.average_reduction.is_finite() && persisted.average_reduction > 0.0 {
            ((persisted.bytes_avoided as f64) / (persisted.average_reduction / 100.0)) as i64
        } else {
            0
        };
        let successful_requests = if persisted.successful_requests > 0 {
            persisted.successful_requests
        } else {
            persisted.files_avoided
        };
        Self {
            successful_requests,
            files_avoided: persisted.files_avoided,
            lines_avoided: persisted.lines_avoided,
            bytes_avoided: persisted.bytes_avoided,
            total_original_bytes,
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
        let last_persisted_request_count = accumulator.successful_requests;
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
    pub fn record(&self, sample: RequestSample) {
        let (persisted, request_count) = {
            let mut accumulator = self
                .accumulator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            accumulator.record(sample);
            (
                PersistedLifetimeStatistics::from(*accumulator),
                accumulator.successful_requests,
            )
        };
        self.persist(&persisted, request_count, false);
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
        let persisted = {
            let mut accumulator = self
                .accumulator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *accumulator = LifetimeAccumulator::default();
            PersistedLifetimeStatistics::from(*accumulator)
        };
        self.persist(&persisted, 0, true);
    }

    fn persist(&self, persisted: &PersistedLifetimeStatistics, request_count: u64, force: bool) {
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
        let Ok(contents) = serde_json::to_string_pretty(persisted) else {
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
    as_i64(original).saturating_sub(as_i64(returned))
}

fn as_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[allow(clippy::cast_precision_loss)]
fn weighted_reduction(bytes_avoided: i64, total_original_bytes: i64) -> f64 {
    if total_original_bytes <= 0 {
        return 0.0;
    }
    (bytes_avoided as f64 / total_original_bytes as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{LifetimeStatistics, RequestSample, SessionStatistics, SourceMetrics};

    static NEXT_TEST_PATH: AtomicU64 = AtomicU64::new(0);

    fn sample(
        original_bytes: u64,
        original_lines: u64,
        returned_bytes: u64,
        returned_lines: u64,
        files: u64,
    ) -> RequestSample {
        RequestSample {
            original: SourceMetrics {
                bytes: original_bytes,
                lines: original_lines,
            },
            returned: SourceMetrics {
                bytes: returned_bytes,
                lines: returned_lines,
            },
            files,
        }
    }

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
    fn records_session_aggregate_avoidance_and_weighted_reduction() {
        let statistics = SessionStatistics::default();
        statistics.record(sample(100, 20, 20, 5, 1));
        statistics.record(sample(200, 40, 100, 20, 1));

        assert_eq!(statistics.snapshot().successful_requests, 2);
        assert_eq!(statistics.snapshot().files_avoided, 2);
        assert_eq!(statistics.snapshot().lines_avoided, 35);
        assert_eq!(statistics.snapshot().bytes_avoided, 180);
        assert_eq!(statistics.snapshot().estimated_token_savings, 45);
        // Size-weighted: 180 avoided out of 300 original = 60%.
        assert!(
            (statistics.snapshot().average_context_reduction_percent - 60.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn counts_distinct_files_separately_from_requests() {
        let statistics = SessionStatistics::default();
        statistics.record(sample(500, 100, 50, 10, 7));
        assert_eq!(statistics.snapshot().successful_requests, 1);
        assert_eq!(statistics.snapshot().files_avoided, 7);
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
        statistics.record(sample(100, 20, 20, 5, 1));
        let persisted = fs::read_to_string(&path).expect("statistics should be persisted");
        assert!(persisted.contains("filesAvoided"));
        assert!(persisted.contains("successfulRequests"));
        assert!(persisted.contains("totalOriginalBytes"));
        assert!(persisted.contains("estimatedTokensAvoided"));

        let loaded = LifetimeStatistics::from_path(path.clone());
        assert_eq!(loaded.snapshot().successful_requests, 1);
        assert_eq!(loaded.snapshot().files_avoided, 1);
        assert_eq!(loaded.snapshot().bytes_avoided, 80);
        assert_eq!(loaded.snapshot().estimated_token_savings, 20);
        // 80 of 100 original bytes.
        assert!((loaded.snapshot().average_context_reduction_percent - 80.0).abs() < f64::EPSILON);

        loaded.reset();
        let reset = LifetimeStatistics::from_path(path.clone());
        assert_eq!(reset.snapshot().successful_requests, 0);
        assert_eq!(reset.snapshot().files_avoided, 0);
        assert_eq!(reset.snapshot().bytes_avoided, 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_persistence_disables_writes_without_affecting_memory() {
        let path = temporary_statistics_path();
        fs::write(&path, "not-json").expect("test statistics file should be writable");
        let statistics = LifetimeStatistics::from_path(path.clone());
        statistics.record(sample(100, 20, 20, 5, 1));
        assert_eq!(statistics.snapshot().files_avoided, 1);
        assert_eq!(
            fs::read_to_string(&path).expect("file should remain readable"),
            "not-json"
        );
        let _ = fs::remove_file(path);
    }
}
