//! End-to-end tests for `frostx run` using hooks (no external tooling required).

use std::fs;
use tempfile::tempdir;

fn frostx_bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.pop();
    p.push("frostx");
    p
}

fn run_cmd(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    std::process::Command::new(frostx_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run frostx")
}

/// Write a frostx.toml with a rule triggered immediately (after=1h, file older than 2h)
/// and a single hook action.
fn write_triggered_config(dir: &std::path::Path, hook_cmd: &str) {
    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000001"

[config.hook.test_action]
command = "{hook_cmd}"
kind = "mutation"

[[rule]]
after = "1h"
actions = ["hook.test_action"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

fn write_check_hook_config(dir: &std::path::Path, hook_cmd: &str) {
    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000002"

[config.hook.my_check]
command = "{hook_cmd}"
kind = "check"

[[rule]]
after = "1h"
actions = ["hook.my_check"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

fn make_old_file(dir: &std::path::Path) {
    // Write a file and backdate it using `touch`.
    let path = dir.join("old_file.txt");
    fs::write(&path, "old content").unwrap();
    // Set mtime to 48 hours ago.
    std::process::Command::new("touch")
        .args(["-d", "48 hours ago", path.to_str().unwrap()])
        .output()
        .unwrap();
}

#[test]
fn run_dry_run_does_not_execute_hook() {
    let tmp = tempdir().unwrap();
    let marker = tmp.path().join("executed");
    make_old_file(tmp.path());
    write_triggered_config(tmp.path(), &format!("touch {}", marker.display()));
    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--dry-run",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!marker.exists(), "hook must not run in dry-run mode");
}

#[test]
fn run_executes_hook_when_triggered() {
    let tmp = tempdir().unwrap();
    let marker = tmp.path().join("hook_ran");
    make_old_file(tmp.path());
    write_triggered_config(tmp.path(), &format!("touch {}", marker.display()));
    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(marker.exists(), "hook should have created the marker file");
}

#[test]
fn run_check_hook_failure_stops_chain() {
    let tmp = tempdir().unwrap();
    let marker = tmp.path().join("should_not_exist");
    make_old_file(tmp.path());
    // First action: check that fails. Second action: hook that creates marker.
    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000003"

[config.hook.fail_check]
command = "exit 1"
kind = "check"

[config.hook.marker]
command = "touch {}"
kind = "mutation"

[[rule]]
after = "1h"
actions = ["hook.fail_check", "hook.marker"]
"#,
        marker.display()
    );
    fs::write(tmp.path().join("frostx.toml"), &config).unwrap();

    let state_dir = tempdir().unwrap();
    run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        !marker.exists(),
        "second action must be skipped after check failure"
    );
}

#[test]
fn run_mutation_recorded_and_skipped_on_rerun() {
    let tmp = tempdir().unwrap();
    let counter = tmp.path().join("count.txt");
    make_old_file(tmp.path());
    write_triggered_config(tmp.path(), &format!("echo x >> {}", counter.display()));
    let state_dir = tempdir().unwrap();
    let args = [
        "--yes",
        "--state-dir",
        state_dir.path().to_str().unwrap(),
        "run",
        ".",
    ];

    run_cmd(&args, tmp.path());
    run_cmd(&args, tmp.path());

    // Hook should have run only once - second run skips because it's completed.
    let content = fs::read_to_string(&counter).unwrap_or_default();
    assert_eq!(
        content.lines().count(),
        1,
        "mutation should run exactly once without --force"
    );
}

#[test]
fn run_force_reruns_completed_mutation() {
    let tmp = tempdir().unwrap();
    // Write counter outside the project dir so it doesn't reset the inactivity clock.
    let counter_dir = tempdir().unwrap();
    let counter = counter_dir.path().join("count.txt");
    make_old_file(tmp.path());
    write_triggered_config(tmp.path(), &format!("echo x >> {}", counter.display()));
    let state_dir = tempdir().unwrap();

    run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );
    run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            "--force",
            ".",
        ],
        tmp.path(),
    );

    let content = fs::read_to_string(&counter).unwrap_or_default();
    assert_eq!(
        content.lines().count(),
        2,
        "--force should re-run the mutation"
    );
}

#[test]
fn run_json_ndjson_output() {
    let tmp = tempdir().unwrap();
    make_old_file(tmp.path());
    write_check_hook_config(tmp.path(), "true");
    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Each line should be valid JSON.
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("invalid NDJSON line '{line}': {e}"));
        assert!(v["action"].is_string());
        assert!(v["status"].is_string());
    }
}

#[test]
fn failed_rule_blocks_subsequent_rules() {
    let tmp = tempdir().unwrap();
    let rule2_marker = tmp.path().join("rule2_ran");
    make_old_file(tmp.path());

    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000010"

[config.hook.fail_check]
command = "exit 1"
kind = "check"

[config.hook.rule2_action]
command = "touch {}"
kind = "mutation"

[[rule]]
after = "1h"
actions = ["hook.fail_check"]

[[rule]]
after = "1h"
actions = ["hook.rule2_action"]
"#,
        rule2_marker.display()
    );
    fs::write(tmp.path().join("frostx.toml"), &config).unwrap();

    let state_dir = tempdir().unwrap();
    run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            ".",
        ],
        tmp.path(),
    );

    assert!(
        !rule2_marker.exists(),
        "rule 2 must be skipped when rule 1 fails"
    );
}

#[test]
fn failed_rule_retried_on_next_run_and_unblocks_subsequent() {
    let tmp = tempdir().unwrap();
    // Keep gate and marker outside the project dir so writing them does not
    // reset the inactivity clock (scanner uses project dir mtime).
    let external = tempdir().unwrap();
    let gate = external.path().join("gate");
    let rule2_marker = external.path().join("rule2_ran");
    make_old_file(tmp.path());

    // gate file absent  => hook exits 1 (fail); present => exits 0 (pass)
    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000011"

[config.hook.gate_check]
command = "test -f {gate}"
kind = "check"

[config.hook.rule2_action]
command = "touch {marker}"
kind = "mutation"

[[rule]]
after = "1h"
actions = ["hook.gate_check"]

[[rule]]
after = "1h"
actions = ["hook.rule2_action"]
"#,
        gate = gate.display(),
        marker = rule2_marker.display()
    );
    fs::write(tmp.path().join("frostx.toml"), &config).unwrap();

    let state_dir = tempdir().unwrap();
    let args = [
        "--yes",
        "--state-dir",
        state_dir.path().to_str().unwrap(),
        "run",
        ".",
    ];

    // First run: gate absent, rule 1 fails, rule 2 blocked.
    run_cmd(&args, tmp.path());
    assert!(
        !rule2_marker.exists(),
        "rule 2 must be blocked on first run"
    );

    // Second run: gate present, rule 1 passes, rule 2 runs.
    fs::write(&gate, "").unwrap();
    run_cmd(&args, tmp.path());
    assert!(
        rule2_marker.exists(),
        "rule 2 must run once rule 1 succeeds"
    );
}

#[test]
fn run_action_filter_runs_single_action() {
    let tmp = tempdir().unwrap();
    // Counter outside project dir to avoid resetting the inactivity clock.
    let counter_dir = tempdir().unwrap();
    let counter = counter_dir.path().join("count.txt");
    make_old_file(tmp.path());
    write_triggered_config(tmp.path(), &format!("echo x >> {}", counter.display()));
    let state_dir = tempdir().unwrap();

    // Use --action to run hook.test_action directly (bypasses time threshold).
    run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            "--action",
            "hook.test_action",
            ".",
        ],
        tmp.path(),
    );

    let content = fs::read_to_string(&counter).unwrap_or_default();
    assert_eq!(
        content.lines().count(),
        1,
        "--action filter should run the named action once"
    );
}
