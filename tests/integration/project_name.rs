//! Integration tests for the optional project `name` field.

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

fn write_named_project_config(dir: &std::path::Path, name: &str) {
    let config = format!(
        r#"id = "c3d4e5f6-0000-0000-0000-000000000077"
name = "{name}"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

fn write_unnamed_project_config(dir: &std::path::Path) {
    let config = r#"id = "c3d4e5f6-0000-0000-0000-000000000078"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#;
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

#[test]
fn check_json_includes_project_name_when_set() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_named_project_config(tmp.path(), "My Cool Project");
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
        v["project"].as_str(),
        Some("My Cool Project"),
        "project name missing from JSON output; got: {stdout}"
    );
}

#[test]
fn check_human_output_shows_project_name_when_set() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), "content").unwrap();
    write_named_project_config(tmp.path(), "My Cool Project");
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
        stdout.contains("My Cool Project"),
        "project name not shown in human output; got:\n{stdout}"
    );
}

#[test]
fn check_json_falls_back_to_dir_name_when_no_project_name() {
    let tmp = tempdir().unwrap();
    let project_dir = tmp.path().join("my-dir-name");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join("file.txt"), "content").unwrap();
    write_unnamed_project_config(&project_dir);
    let state_dir = tempdir().unwrap();

    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        &project_dir,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");
    assert_eq!(
        v["project"].as_str(),
        Some("my-dir-name"),
        "expected directory name as fallback; got: {stdout}"
    );
}

#[test]
fn name_field_not_written_to_toml_when_absent() {
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
    // Only check the header section before any [[rule]] blocks so that rule `name =`
    // fields do not produce a false positive.
    let header = content.split("[[rule]]").next().unwrap_or(&content);
    assert!(
        !header.contains("name ="),
        "top-level name field should not appear in generated frostx.toml when not set; got:\n{content}"
    );
}
