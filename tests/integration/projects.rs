//! End-to-end tests for `frostx projects`.

use std::fs;
use tempfile::tempdir;

fn frostx_bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.pop();
    p.push("frostx");
    p
}

fn run(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    std::process::Command::new(frostx_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run frostx")
}

fn init_project(dir: &std::path::Path, state_dir: &std::path::Path) {
    run(&["init", "."], dir);
    // register in state via check
    run(
        &["--state-dir", state_dir.to_str().unwrap(), "check", "."],
        dir,
    );
}

// ── list ─────────────────────────────────────────────────────────────────────

#[test]
fn projects_list_empty_state_dir() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "list",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn projects_list_json_shape() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_project(proj.path(), state_dir.path());

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "list",
        ],
        proj.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(v["projects"].is_array());
    assert_eq!(v["projects"].as_array().unwrap().len(), 1);
    assert!(v["projects"][0]["uuid"].is_string());
    assert!(v["projects"][0]["path"].is_string());
}

// ── add ───────────────────────────────────────────────────────────────────────

#[test]
fn projects_add_registers_project() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    run(&["init", "."], proj.path());

    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now list should show it.
    let list_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "list",
        ],
        proj.path(),
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&list_out.stdout)).unwrap();
    assert_eq!(v["projects"].as_array().unwrap().len(), 1);
}

#[test]
fn projects_add_json_shape() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    run(&["init", "."], proj.path());

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(v["added"].is_array());
    assert_eq!(v["added"].as_array().unwrap().len(), 1);
    assert!(v["skipped"].is_array());
    assert!(v["added"][0]["uuid"].is_string());
    assert!(v["added"][0]["path"].is_string());
}

#[test]
fn projects_add_scan_finds_nested_projects() {
    let state_dir = tempdir().unwrap();
    let root = tempdir().unwrap();

    // Create two projects inside root.
    let proj_a = root.path().join("a");
    let proj_b = root.path().join("b");
    fs::create_dir(&proj_a).unwrap();
    fs::create_dir(&proj_b).unwrap();
    run(&["init", "."], &proj_a);
    run(&["init", "."], &proj_b);

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            "--scan",
            root.path().to_str().unwrap(),
        ],
        root.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(
        v["added"].as_array().unwrap().len(),
        2,
        "--scan should find both projects"
    );
}

#[test]
fn projects_add_uninitialized_path_is_skipped() {
    let state_dir = tempdir().unwrap();
    let noproject = tempdir().unwrap();

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            noproject.path().to_str().unwrap(),
        ],
        noproject.path(),
    );
    // Command itself succeeds but reports the path in skipped.
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["added"].as_array().unwrap().len(), 0);
    assert_eq!(v["skipped"].as_array().unwrap().len(), 1);
}

// ── rm ────────────────────────────────────────────────────────────────────────

#[test]
fn projects_rm_removes_project() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_project(proj.path(), state_dir.path());

    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "rm",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // List should now be empty.
    let list_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "list",
        ],
        proj.path(),
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&list_out.stdout)).unwrap();
    assert!(v["projects"].as_array().unwrap().is_empty());
}

#[test]
fn projects_rm_json_shape() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_project(proj.path(), state_dir.path());

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "rm",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(v["uuid"].is_string());
    assert!(v["path"].is_string());
}

// ── check (all) ───────────────────────────────────────────────────────────────

#[test]
fn projects_check_outputs_each_tracked_project() {
    let state_dir = tempdir().unwrap();
    let proj_a = tempdir().unwrap();
    let proj_b = tempdir().unwrap();
    init_project(proj_a.path(), state_dir.path());
    init_project(proj_b.path(), state_dir.path());

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
        ],
        proj_a.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(v.is_array(), "should be a JSON array");
    assert_eq!(
        v.as_array().unwrap().len(),
        2,
        "should report both tracked projects"
    );
}

#[test]
fn projects_check_empty_registry_succeeds() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(v.is_array());
    assert!(v.as_array().unwrap().is_empty());
}

// ── run (all) ─────────────────────────────────────────────────────────────────

#[test]
fn projects_run_includes_project_field_in_ndjson() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();

    // Write a config with a triggered hook.
    let config = r#"id = "a1b2c3d4-0000-0000-0000-aabbccddeeff"

[config.hook.check_ok]
command = "true"
kind = "check"

[[rule]]
after = "1h"
actions = ["hook.check_ok"]
"#;
    std::fs::write(proj.path().join("frostx.toml"), config).unwrap();

    // Backdate a file so the rule triggers.
    let old_file = proj.path().join("old.txt");
    std::fs::write(&old_file, "x").unwrap();
    std::process::Command::new("touch")
        .args(["-d", "48 hours ago", old_file.to_str().unwrap()])
        .output()
        .unwrap();

    // Register the project.
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );

    let out = run(
        &[
            "--json",
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
        ],
        proj.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid NDJSON: {line}: {e}"));
        assert!(
            v["project"].is_string(),
            "each NDJSON line must have 'project' field"
        );
    }
}

// ── --daily (non-TTY behaviour) ───────────────────────────────────────────────
//
// Integration tests run without a TTY, so `--daily` always takes the
// early-exit path.  These tests verify:
//  1. `--daily` (human) is silent; `--daily --json` always emits an envelope.
//  2. The envelope carries `daily_source`: "not-run", "cached", or "fresh".
//  3. The check cache and run cache are fully independent.
//
// Cache helpers store the full envelope JSON (with "cached" source) because
// that is exactly what the binary stores on a real fresh run.

fn daily_check_envelope(
    daily_source: frostx::output::DailySource,
    results: &[frostx::output::CheckOutput],
) -> String {
    serde_json::to_string(&frostx::output::DailyCheckOutput {
        frostx_version: frostx::output::FROSTX_VERSION,
        daily_source,
        results,
    })
    .expect("DailyCheckOutput serialization must not fail")
}

fn daily_run_envelope(
    daily_source: frostx::output::DailySource,
    actions: &[frostx::output::RunActionOutput],
) -> String {
    serde_json::to_string(&frostx::output::DailyRunOutput {
        frostx_version: frostx::output::FROSTX_VERSION,
        daily_source,
        actions,
    })
    .expect("DailyRunOutput serialization must not fail")
}

fn write_daily_check_cache(state_dir: &std::path::Path, envelope: &str) {
    let mut daily = frostx::config::daily::DailyState::load(state_dir).unwrap_or_default();
    daily.record_check(Some(envelope.to_string()));
    daily.save(state_dir).unwrap();
}

fn write_daily_run_cache(state_dir: &std::path::Path, envelope: &str) {
    let mut daily = frostx::config::daily::DailyState::load(state_dir).unwrap_or_default();
    daily.record_run(Some(envelope.to_string()));
    daily.save(state_dir).unwrap();
}

fn parse_daily_json(output: &[u8]) -> serde_json::Value {
    let s = String::from_utf8_lossy(output);
    serde_json::from_str(s.trim())
        .unwrap_or_else(|e| panic!("--daily --json output must be valid JSON: {e}\nGot: {s}"))
}

// Human mode: silent when there is no cache and the TTY check suppresses the run.

#[test]
fn projects_check_daily_human_is_silent_without_cache() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "human mode should be silent");
}

#[test]
fn projects_run_daily_human_is_silent_without_cache() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "human mode should be silent");
}

// JSON mode: "not-run" envelope when no cache exists.

#[test]
fn projects_check_daily_json_emits_not_run_when_no_cache() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v = parse_daily_json(&out.stdout);
    assert_eq!(v["daily_source"], "not-run");
    assert_eq!(v["results"], serde_json::json!([]));
}

#[test]
fn projects_run_daily_json_emits_not_run_when_no_cache() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v = parse_daily_json(&out.stdout);
    assert_eq!(v["daily_source"], "not-run");
    assert_eq!(v["actions"], serde_json::json!([]));
}

// JSON mode: cached envelope returned verbatim when cache exists.

#[test]
fn projects_check_daily_json_returns_cached_envelope() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let envelope = daily_check_envelope(frostx::output::DailySource::Cached, &[]);
    write_daily_check_cache(state_dir.path(), &envelope);

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{envelope}\n"),
        "cached envelope must be returned verbatim with trailing newline"
    );
}

#[test]
fn projects_run_daily_json_returns_cached_envelope() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();
    let envelope = daily_run_envelope(frostx::output::DailySource::Cached, &[]);
    write_daily_run_cache(state_dir.path(), &envelope);

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{envelope}\n"),
        "cached envelope must be returned verbatim with trailing newline"
    );
}

// Separation: check cache must not appear in run output and vice versa.

#[test]
fn projects_daily_check_cache_does_not_bleed_into_run() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();

    write_daily_check_cache(
        state_dir.path(),
        &daily_check_envelope(frostx::output::DailySource::Cached, &[]),
    );

    // No run cache → run should emit "not-run" envelope.
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v = parse_daily_json(&out.stdout);
    assert_eq!(
        v["daily_source"], "not-run",
        "run should not use the check cache"
    );
}

#[test]
fn projects_daily_run_cache_does_not_bleed_into_check() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();

    write_daily_run_cache(
        state_dir.path(),
        &daily_run_envelope(frostx::output::DailySource::Cached, &[]),
    );

    // No check cache → check should emit "not-run" envelope.
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v = parse_daily_json(&out.stdout);
    assert_eq!(
        v["daily_source"], "not-run",
        "check should not use the run cache"
    );
}

#[test]
fn projects_daily_both_caches_are_returned_independently() {
    let state_dir = tempdir().unwrap();
    let tmp = tempdir().unwrap();

    let check_envelope = daily_check_envelope(frostx::output::DailySource::Cached, &[]);
    let run_envelope = daily_run_envelope(frostx::output::DailySource::Cached, &[]);
    {
        let mut daily = frostx::config::daily::DailyState::default();
        daily.record_check(Some(check_envelope.clone()));
        daily.record_run(Some(run_envelope.clone()));
        daily.save(state_dir.path()).unwrap();
    }

    let check_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert_eq!(
        String::from_utf8_lossy(&check_out.stdout),
        format!("{check_envelope}\n")
    );

    let run_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert_eq!(
        String::from_utf8_lossy(&run_out.stdout),
        format!("{run_envelope}\n")
    );
}

// ── daily cache reset on project list change ──────────────────────────────────

#[test]
fn projects_add_resets_daily_cache() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    run(&["init", "."], proj.path());

    // Pre-populate both caches.
    write_daily_check_cache(
        state_dir.path(),
        &daily_check_envelope(frostx::output::DailySource::Cached, &[]),
    );
    write_daily_run_cache(
        state_dir.path(),
        &daily_run_envelope(frostx::output::DailySource::Cached, &[]),
    );

    // Adding a project must clear the cache.
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );

    // Both caches must now be gone → "not-run" envelopes returned.
    let check_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        proj.path(),
    );
    assert_eq!(
        parse_daily_json(&check_out.stdout)["daily_source"],
        "not-run",
        "check cache should be cleared after projects add"
    );

    let run_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        proj.path(),
    );
    assert_eq!(
        parse_daily_json(&run_out.stdout)["daily_source"],
        "not-run",
        "run cache should be cleared after projects add"
    );
}

#[test]
fn projects_rm_resets_daily_cache() {
    let state_dir = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_project(proj.path(), state_dir.path());

    // Pre-populate both caches.
    write_daily_check_cache(
        state_dir.path(),
        &daily_check_envelope(frostx::output::DailySource::Cached, &[]),
    );
    write_daily_run_cache(
        state_dir.path(),
        &daily_run_envelope(frostx::output::DailySource::Cached, &[]),
    );

    // Removing the project must clear the cache.
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "rm",
            proj.path().to_str().unwrap(),
        ],
        proj.path(),
    );

    let tmp = tempdir().unwrap();
    let check_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        tmp.path(),
    );
    assert_eq!(
        parse_daily_json(&check_out.stdout)["daily_source"],
        "not-run",
        "check cache should be cleared after projects rm"
    );

    let run_out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "run",
            "--daily",
        ],
        tmp.path(),
    );
    assert_eq!(
        parse_daily_json(&run_out.stdout)["daily_source"],
        "not-run",
        "run cache should be cleared after projects rm"
    );
}

#[test]
fn projects_add_with_no_new_projects_does_not_reset_cache() {
    let state_dir = tempdir().unwrap();
    let noproject = tempdir().unwrap();

    // Pre-populate check cache with a known envelope.
    let envelope = daily_check_envelope(frostx::output::DailySource::Cached, &[]);
    write_daily_check_cache(state_dir.path(), &envelope);

    // Attempting to add a non-project path leaves all entries in skipped;
    // the list does not change so the cache must be preserved.
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "add",
            noproject.path().to_str().unwrap(),
        ],
        noproject.path(),
    );

    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "projects",
            "check",
            "--daily",
        ],
        noproject.path(),
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{envelope}\n"),
        "cache should be preserved when no project was actually added"
    );
}
