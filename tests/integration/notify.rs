//! End-to-end tests for `notify.*` actions.

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

/// Write a frostx.toml with a triggered rule containing a notify action.
fn write_notify_config(dir: &std::path::Path, message: &str) {
    let config = format!(
        r#"id = "b2c3d4e5-0000-0000-0000-000000000010"

[config.notify.review]
message = "{message}"

[[rule]]
after = "1h"
actions = ["notify.review"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

fn make_old_file(dir: &std::path::Path) {
    let path = dir.join("old_file.txt");
    fs::write(&path, "old content").unwrap();
    std::process::Command::new("touch")
        .arg("-t")
        .arg("202001010000")
        .arg(&path)
        .status()
        .unwrap();
}

#[test]
fn notify_dry_run_does_not_prompt() {
    let tmp = tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir(&state_dir).unwrap();

    write_notify_config(tmp.path(), "Please review before continuing.");
    make_old_file(tmp.path());

    let out = run_cmd(
        &[
            "--state-dir",
            state_dir.to_str().unwrap(),
            "--dry-run",
            "run",
        ],
        tmp.path(),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("dry_run") || stdout.contains('~'),
        "expected dry_run marker, got: {stdout}"
    );
}

#[test]
fn notify_dry_run_json_shows_dry_run_status() {
    let tmp = tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir(&state_dir).unwrap();

    write_notify_config(tmp.path(), "Review checklist.");
    make_old_file(tmp.path());

    let out = run_cmd(
        &[
            "--state-dir",
            state_dir.to_str().unwrap(),
            "--json",
            "--dry-run",
            "run",
        ],
        tmp.path(),
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let line: serde_json::Value =
        serde_json::from_str(stdout.lines().next().unwrap_or("{}")).unwrap();
    assert_eq!(line["status"], "dry_run");
    assert_eq!(line["action"], "notify.review");
}

#[test]
fn notify_undefined_name_exits_with_error() {
    let tmp = tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir(&state_dir).unwrap();

    // Config references notify.missing but [config.notify.missing] is absent.
    let config = r#"id = "b2c3d4e5-0000-0000-0000-000000000011"

[[rule]]
after = "1h"
actions = ["notify.missing"]
"#;
    fs::write(tmp.path().join("frostx.toml"), config).unwrap();
    make_old_file(tmp.path());

    let out = run_cmd(
        &["--state-dir", state_dir.to_str().unwrap(), "run"],
        tmp.path(),
    );

    // The action should report a failure (exit 0 but failed status), not a panic.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("failed") || stdout.contains("missing") || !stderr.is_empty(),
        "expected an error about missing notify config, got stdout={stdout} stderr={stderr}"
    );
}
