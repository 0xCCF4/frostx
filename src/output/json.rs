use super::{
    CheckOutput, DailyCheckOutput, DailyRunOutput, DoctorOutput, GcOutput, InitOutput,
    ProjectAddOutput, ProjectRmOutput, ProjectsListOutput, RunActionOutput, FROSTX_VERSION,
};
use serde_json::json;

/// Emit the JSON result of `frostx init` to stdout.
pub fn print_init(out: &InitOutput) {
    let v = json!({
        "frostx_version": FROSTX_VERSION,
        "path": out.path.display().to_string(),
        "uuid": out.uuid.to_string(),
    });
    println!("{v}");
}

/// Emit the JSON result of `frostx check` to stdout.
pub fn print_check(out: &CheckOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit one NDJSON line for a single action result from `frostx run`.
pub fn print_run_action(out: &RunActionOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the JSON result of `frostx doctor` to stdout.
pub fn print_doctor(out: &DoctorOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the JSON result of `frostx gc` to stdout.
pub fn print_gc(out: &GcOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit an error object to stderr.
pub fn print_error(message: &str, code: i32) {
    let v = json!({
        "frostx_version": FROSTX_VERSION,
        "error": message,
        "code": code,
    });
    eprintln!("{v}");
}

/// Emit a JSON array of check results (used by `frostx scan` and `projects check`).
pub fn print_scan(items: &[CheckOutput]) {
    println!(
        "{}",
        serde_json::to_string(items)
            .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the JSON result of `frostx projects list` to stdout.
pub fn print_projects_list(out: &ProjectsListOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the JSON result of `frostx projects add` to stdout.
pub fn print_projects_add(out: &ProjectAddOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the `--daily --json` envelope for `projects check`.
pub fn print_daily_check(out: &DailyCheckOutput<'_>) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the `--daily --json` envelope for `projects run`.
pub fn print_daily_run(out: &DailyRunOutput<'_>) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}

/// Emit the JSON result of `frostx projects rm` to stdout.
pub fn print_projects_rm(out: &ProjectRmOutput) {
    println!(
        "{}",
        serde_json::to_string(out).unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
    );
}
