//! Integration tests for optional rule `name` field.

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

/// Write a `frostx.toml` with a named rule (not yet triggered — no old files needed).
fn write_named_rule_config(dir: &std::path::Path) {
    let config = r#"id = "b2c3d4e5-0000-0000-0000-000000000099"

[[rule]]
name = "safety checks"
after = "90d"
actions = ["git.check_clean"]
"#;
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

#[test]
fn check_json_includes_rule_name() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_named_rule_config(tmp.path());
    let state_dir = tempdir().unwrap();

    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");
    assert_eq!(
        v["rules"][0]["name"].as_str(),
        Some("safety checks"),
        "rule name missing from JSON output"
    );
}

#[test]
fn check_human_output_includes_rule_name() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_named_rule_config(tmp.path());
    let state_dir = tempdir().unwrap();

    let out = run_cmd(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(safety checks)"),
        "rule name not shown in human output; got:\n{stdout}"
    );
}

#[test]
fn unnamed_rule_json_omits_name_field() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    let config = r#"id = "b2c3d4e5-0000-0000-0000-000000000098"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#;
    fs::write(tmp.path().join("frostx.toml"), config).unwrap();
    let state_dir = tempdir().unwrap();

    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");
    assert!(
        v["rules"][0]["name"].is_null(),
        "unnamed rule should omit 'name' key from JSON"
    );
}
