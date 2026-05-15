//! Integration tests for `--pretend-inactive`.

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

/// Write a `frostx.toml` with a single rule that requires 90 days of inactivity.
fn write_config(dir: &std::path::Path) {
    let config = r#"id = "b1b2c3d4-0000-0000-0000-000000000030"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#;
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

#[test]
fn check_not_triggered_without_flag() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "recent").unwrap();
    write_config(dir.path());

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Rule should not be triggered for a fresh file.
    assert!(
        stdout.contains("\"triggered\":false"),
        "expected no trigger, got: {stdout}"
    );
}

#[test]
fn check_triggered_with_pretend_inactive() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "recent").unwrap();
    write_config(dir.path());

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--pretend-inactive",
            "100d",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Rule should be triggered because 100d > 90d threshold.
    assert!(
        stdout.contains("\"triggered\":true"),
        "expected trigger, got: {stdout}"
    );
}

#[test]
fn run_triggered_with_pretend_inactive() {
    // Uses dry-run so no git action is actually executed.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "recent").unwrap();
    write_config(dir.path());

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--pretend-inactive",
            "100d",
            "--dry-run",
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx run --dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The action should appear in dry-run output.
    assert!(
        stdout.contains("git.check_clean"),
        "expected action in output, got: {stdout}"
    );
}

#[test]
fn invalid_duration_exits_with_error() {
    let dir = tempdir().unwrap();
    write_config(dir.path());

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--pretend-inactive",
            "not-a-duration",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        !out.status.success(),
        "expected non-zero exit for invalid duration"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stderr.contains("pretend-inactive") || stdout.contains("pretend-inactive"),
        "expected error message mentioning pretend-inactive, stderr: {stderr}, stdout: {stdout}"
    );
}
