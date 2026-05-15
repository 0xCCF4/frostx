//! Once-per-day automation state stored in `$state_dir/daily.toml`.
//!
//! When the user invokes `frostx projects run --daily` or
//! `frostx projects check --daily`, the binary reads this file to decide
//! whether the daily run threshold has already been met and writes the
//! timestamp back after a successful run.  The two commands maintain
//! independent timestamps so using one does not suppress the other.
//!
//! The last JSON output of each command is also cached here so that
//! machine-readable callers receive the same data on subsequent invocations
//! within the same 24-hour window (when the command would otherwise be a
//! silent no-op).

use crate::error::FrostxError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

const DAILY_STATE_FILE: &str = "daily.toml";
const SECONDS_PER_DAY: i64 = 86_400;

/// Tracks when each daily automation command last ran, plus its cached output.
///
/// `last_run` / `run_output_json` and `last_check` / `check_output_json` are
/// fully independent so that `projects run --daily` and
/// `projects check --daily` each maintain their own 24-hour cooldown and
/// output cache.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyState {
    /// Timestamp of the most recent `projects run --daily` invocation.
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
    /// Timestamp of the most recent `projects check --daily` invocation.
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
    /// Cached NDJSON output from the most recent `projects run --daily`.
    ///
    /// Each line is one JSON-serialized `RunActionOutput` object.
    #[serde(default)]
    pub run_output_json: Option<String>,
    /// Cached JSON output from the most recent `projects check --daily`.
    ///
    /// A JSON array of serialized `CheckOutput` objects.
    #[serde(default)]
    pub check_output_json: Option<String>,
}

impl DailyState {
    /// Load the daily state from `state_dir/daily.toml`, or return a fresh default.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(state_dir: &Path) -> Result<Self, FrostxError> {
        let path = state_dir.join(DAILY_STATE_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content)
            .map_err(|e| FrostxError::Config(format!("daily state parse error: {e}")))
    }

    /// Persist the daily state to `state_dir/daily.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or the file cannot be written.
    pub fn save(&self, state_dir: &Path) -> Result<(), FrostxError> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join(DAILY_STATE_FILE);
        let content = toml::to_string_pretty(self)
            .map_err(|e| FrostxError::Config(format!("daily state serialization error: {e}")))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Returns `true` if the last `projects run --daily` was within the last 24 hours.
    #[must_use]
    pub fn ran_today(&self) -> bool {
        within_24h(self.last_run)
    }

    /// Returns `true` if the last `projects check --daily` was within the last 24 hours.
    #[must_use]
    pub fn checked_today(&self) -> bool {
        within_24h(self.last_check)
    }

    /// Record `projects run --daily` as having run now, storing its NDJSON output.
    pub fn record_run(&mut self, ndjson: Option<String>) {
        self.last_run = Some(Utc::now());
        self.run_output_json = ndjson;
    }

    /// Record `projects check --daily` as having run now, storing its JSON output.
    pub fn record_check(&mut self, json: Option<String>) {
        self.last_check = Some(Utc::now());
        self.check_output_json = json;
    }
}

fn within_24h(ts: Option<DateTime<Utc>>) -> bool {
    ts.is_some_and(|t| Utc::now().signed_duration_since(t).num_seconds() < SECONDS_PER_DAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::tempdir;

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempdir().unwrap();
        let mut state = DailyState::default();
        state.record_run(Some("ndjson".into()));
        state.record_check(Some("json".into()));
        state.save(tmp.path()).unwrap();
        let loaded = DailyState::load(tmp.path()).unwrap();
        assert!(loaded.last_run.is_some());
        assert!(loaded.last_check.is_some());
        assert_eq!(loaded.run_output_json.as_deref(), Some("ndjson"));
        assert_eq!(loaded.check_output_json.as_deref(), Some("json"));
    }

    #[test]
    fn timestamps_are_independent_in_memory() {
        let mut state = DailyState::default();
        state.record_run(None);
        assert!(state.ran_today());
        assert!(!state.checked_today());
        state.record_check(None);
        assert!(state.ran_today());
        assert!(state.checked_today());
    }

    /// Verifies that saving only `last_check` and reloading still gives `last_run = None`.
    #[test]
    fn timestamps_are_independent_after_file_roundtrip() {
        let tmp = tempdir().unwrap();

        let mut state = DailyState::default();
        state.record_check(None);
        state.save(tmp.path()).unwrap();

        let loaded = DailyState::load(tmp.path()).unwrap();
        assert!(loaded.checked_today(), "check should be recorded");
        assert!(!loaded.ran_today(), "run should not be recorded");

        let mut state2 = DailyState::load(tmp.path()).unwrap();
        state2.record_run(None);
        state2.save(tmp.path()).unwrap();

        let final_state = DailyState::load(tmp.path()).unwrap();
        assert!(
            final_state.checked_today(),
            "check should still be recorded"
        );
        assert!(final_state.ran_today(), "run should now be recorded");
    }

    #[test]
    fn ran_today_true_when_recent() {
        let state = DailyState {
            last_run: Some(Utc::now() - Duration::hours(1)),
            ..Default::default()
        };
        assert!(state.ran_today());
        assert!(!state.checked_today());
    }

    #[test]
    fn checked_today_true_when_recent() {
        let state = DailyState {
            last_check: Some(Utc::now() - Duration::hours(1)),
            ..Default::default()
        };
        assert!(state.checked_today());
        assert!(!state.ran_today());
    }

    #[test]
    fn ran_today_false_when_old() {
        let state = DailyState {
            last_run: Some(Utc::now() - Duration::hours(25)),
            ..Default::default()
        };
        assert!(!state.ran_today());
    }

    #[test]
    fn checked_today_false_when_old() {
        let state = DailyState {
            last_check: Some(Utc::now() - Duration::hours(25)),
            ..Default::default()
        };
        assert!(!state.checked_today());
    }

    #[test]
    fn defaults_are_false() {
        let state = DailyState::default();
        assert!(!state.ran_today());
        assert!(!state.checked_today());
    }

    #[test]
    fn missing_file_returns_default() {
        let tmp = tempdir().unwrap();
        let state = DailyState::load(tmp.path()).unwrap();
        assert!(state.last_run.is_none());
        assert!(state.last_check.is_none());
    }

    #[test]
    fn cache_fields_round_trip() {
        let tmp = tempdir().unwrap();
        let mut state = DailyState::default();
        state.record_run(Some(r#"{"action":"git.check_clean"}"#.into()));
        state.save(tmp.path()).unwrap();

        let loaded = DailyState::load(tmp.path()).unwrap();
        assert_eq!(
            loaded.run_output_json.as_deref(),
            Some(r#"{"action":"git.check_clean"}"#)
        );
        assert!(loaded.check_output_json.is_none());
    }
}
