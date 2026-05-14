//! Integration tests for the optional project `description` field.

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

fn write_described_project_config(dir: &std::path::Path, description: &str) {
    let config = format!(
        r#"id = "d4e5f6a7-0000-0000-0000-000000000091"
description = "{description}"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

fn write_undescribed_project_config(dir: &std::path::Path) {
    let config = r#"id = "d4e5f6a7-0000-0000-0000-000000000092"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#;
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

#[test]
fn check_json_includes_description_when_set() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_described_project_config(tmp.path(), "Billing backend service");
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
        v["description"].as_str(),
        Some("Billing backend service"),
        "description missing from JSON output; got: {stdout}"
    );
}

#[test]
fn check_human_output_shows_description_when_set() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_described_project_config(tmp.path(), "Billing backend service");
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
        stdout.contains("Billing backend service"),
        "description not shown in human output; got:\n{stdout}"
    );
}

#[test]
fn check_json_omits_description_when_absent() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_undescribed_project_config(tmp.path());
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
    assert!(
        v.get("description").is_none(),
        "description key should be absent from JSON when not set; got: {stdout}"
    );
}

#[test]
fn description_field_not_written_to_toml_when_absent() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();

    run_cmd(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "init",
            ".",
        ],
        tmp.path(),
    );

    let content = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    assert!(
        !content.contains("description ="),
        "description field should not appear in generated frostx.toml when not set; got:\n{content}"
    );
}
