//! Output rendering - human-readable and JSON/NDJSON.

/// Human-readable (colored) renderer.
pub mod human;
/// JSON / NDJSON renderer.
pub mod json;

use crate::pipeline::{ActionStatus, RuleOutcome};
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

/// Selects which renderer is active for this invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

/// Data passed to renderers for `frostx init`.
pub struct InitOutput {
    pub path: std::path::PathBuf,
    pub uuid: Uuid,
}

/// Data passed to renderers for `frostx check`.
#[derive(Serialize)]
pub struct CheckOutput {
    pub frostx_version: &'static str,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub path: String,
    pub uuid: String,
    pub inactive_seconds: i64,
    pub rules: Vec<RuleCheckOutput>,
}

/// Per-rule section of a `check` response.
#[derive(Serialize)]
pub struct RuleCheckOutput {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub after: String,
    pub after_seconds: i64,
    pub triggered: bool,
    pub remaining_seconds: i64,
    pub actions: Vec<ActionCheckOutput>,
}

/// Per-action section within a rule check output.
#[derive(Serialize, Clone)]
pub struct ActionCheckOutput {
    pub name: String,
    pub status: String,
    pub message: String,
}

/// Indicates how the data in a `--daily --json` envelope was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DailySource {
    /// Data was produced by the current invocation.
    Fresh,
    /// Data was retrieved from the 24-hour cache.
    Cached,
    /// `--daily` suppressed the run (non-TTY or threshold not met).
    NotRun,
}

/// Envelope for `frostx projects check --daily --json`.
///
/// Always emitted when `--daily --json` is used, regardless of whether the
/// data is fresh, cached, or suppressed, so callers can reliably parse it.
#[derive(Serialize)]
pub struct DailyCheckOutput<'a> {
    pub frostx_version: &'static str,
    pub daily_source: DailySource,
    pub results: &'a [CheckOutput],
}

/// Envelope for `frostx projects run --daily --json`.
///
/// Always emitted when `--daily --json` is used. Actions are buffered (not
/// streamed) so they can be included in the envelope and cached for replay.
#[derive(Serialize)]
pub struct DailyRunOutput<'a> {
    pub frostx_version: &'static str,
    pub daily_source: DailySource,
    pub actions: &'a [RunActionOutput],
}

/// Data for `frostx run` (streamed per action as NDJSON).
#[derive(Serialize)]
pub struct RunActionOutput {
    pub frostx_version: &'static str,
    /// Project path included when running via `projects run` to distinguish sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub rule: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name: Option<String>,
    pub action: String,
    pub status: String,
    pub message: String,
}

/// Data for `frostx doctor`.
#[derive(Serialize)]
pub struct DoctorOutput {
    pub frostx_version: &'static str,
    pub valid: bool,
    pub errors: Vec<DoctorItem>,
    pub warnings: Vec<DoctorItem>,
}

/// A single error or warning item in a `doctor` response.
#[derive(Serialize)]
pub struct DoctorItem {
    pub field: String,
    pub message: String,
}

/// Data for `frostx gc`.
#[derive(Serialize)]
pub struct GcOutput {
    pub frostx_version: &'static str,
    pub orphaned: Vec<GcEntry>,
    pub removed: usize,
}

/// A single orphaned-state entry in a `gc` response.
#[derive(Serialize)]
pub struct GcEntry {
    pub state_file: String,
    pub reason: String,
    pub path: String,
}

/// A single entry in `frostx projects list`.
#[derive(Serialize)]
pub struct ProjectEntry {
    pub uuid: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan: Option<String>,
}

/// Output for `frostx projects list`.
#[derive(Serialize)]
pub struct ProjectsListOutput {
    pub frostx_version: &'static str,
    pub projects: Vec<ProjectEntry>,
}

/// Output for `frostx projects add`.
#[derive(Serialize)]
pub struct ProjectAddOutput {
    pub frostx_version: &'static str,
    pub added: Vec<ProjectEntry>,
    pub skipped: Vec<ProjectAddSkip>,
}

/// A project skipped during `projects add` with the reason.
#[derive(Serialize)]
pub struct ProjectAddSkip {
    pub path: String,
    pub reason: String,
}

/// Output for `frostx projects rm`.
#[derive(Serialize)]
pub struct ProjectRmOutput {
    pub frostx_version: &'static str,
    pub uuid: String,
    pub path: String,
}

/// The crate version, embedded at compile time for JSON output versioning.
pub const FROSTX_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build `CheckOutput` from pipeline results.
#[must_use]
pub fn build_check_output(
    project_name: &str,
    project_description: Option<&str>,
    project_path: &Path,
    uuid: Uuid,
    inactive_seconds: i64,
    rule_outcomes: &[RuleOutcome],
) -> CheckOutput {
    let rules = rule_outcomes
        .iter()
        .enumerate()
        .map(|(i, ro)| RuleCheckOutput {
            index: i + 1,
            name: ro.name.clone(),
            after: ro.after.to_string(),
            after_seconds: ro.after_seconds,
            triggered: ro.triggered,
            remaining_seconds: ro.remaining_seconds,
            actions: ro
                .action_outcomes
                .iter()
                .map(|ao| ActionCheckOutput {
                    name: ao.name.clone(),
                    status: ao.status.as_str().to_string(),
                    message: ao.message.clone(),
                })
                .collect(),
        })
        .collect();

    CheckOutput {
        frostx_version: FROSTX_VERSION,
        project: project_name.to_string(),
        description: project_description.map(str::to_string),
        path: project_path.display().to_string(),
        uuid: uuid.to_string(),
        inactive_seconds,
        rules,
    }
}

impl ActionStatus {
    /// Stable lowercase string representation used in JSON output.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Completed => "completed",
            Self::DryRun => "dry_run",
        }
    }
}
