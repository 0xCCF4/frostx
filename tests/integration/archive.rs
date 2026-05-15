//! End-to-end tests for `archive.compress` action behavior.

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

/// Write a `frostx.toml` that triggers immediately and runs `archive.compress`.
fn write_archive_config(dir: &std::path::Path) {
    let config = r#"id = "a1b2c3d4-0000-0000-0000-000000000020"

[[rule]]
after = "1h"
actions = ["archive.compress"]
"#;
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

/// Find the single `.tar.gz` file produced by `archive.compress` in `parent`.
fn find_archive(parent: &std::path::Path) -> std::path::PathBuf {
    fs::read_dir(parent)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .find(|e| e.file_name().to_string_lossy().ends_with(".tar.gz"))
        .expect("archive must exist in parent directory")
        .path()
}

fn touch_old(path: &std::path::Path) {
    std::process::Command::new("touch")
        .args(["-d", "48 hours ago", path.to_str().unwrap()])
        .output()
        .unwrap();
}

fn make_old_file(dir: &std::path::Path) {
    let path = dir.join("old_file.txt");
    fs::write(&path, "old content").unwrap();
    std::process::Command::new("touch")
        .args(["-d", "48 hours ago", path.to_str().unwrap()])
        .output()
        .unwrap();
}

#[test]
fn archive_replaces_project_dir() {
    // Layout: parent/project/ - archive lands in parent/, project/ is removed.
    let parent = tempdir().unwrap();
    let project = parent.path().join("myproject");
    fs::create_dir(&project).unwrap();
    make_old_file(&project);
    write_archive_config(&project);

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            project.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "frostx run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Original project directory must be gone.
    assert!(
        !project.exists(),
        "project directory must be replaced by archive"
    );

    // Exactly one archive file must exist in the parent directory.
    let archives: Vec<_> = fs::read_dir(parent.path())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tar.gz"))
        .collect();
    assert_eq!(archives.len(), 1, "expected exactly one archive in parent");
    assert!(
        archives[0].metadata().unwrap().len() > 0,
        "archive must not be empty"
    );
}

#[test]
fn archive_dry_run_keeps_project_dir() {
    let parent = tempdir().unwrap();
    let project = parent.path().join("myproject");
    fs::create_dir(&project).unwrap();
    make_old_file(&project);
    write_archive_config(&project);

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--dry-run",
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            project.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "frostx run --dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(project.exists(), "project dir must survive a dry run");
    let archives: Vec<_> = fs::read_dir(parent.path())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tar.gz"))
        .collect();
    assert!(
        archives.is_empty(),
        "no archive should be created in dry-run"
    );
}

#[test]
fn archive_state_updated_to_archive_path() {
    let parent = tempdir().unwrap();
    let project = parent.path().join("myproject");
    fs::create_dir(&project).unwrap();
    make_old_file(&project);
    write_archive_config(&project);

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            project.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "frostx run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read the state file and verify project_path now points to the archive.
    let state_files: Vec<_> = fs::read_dir(state_dir.path())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".toml"))
        .collect();
    assert_eq!(state_files.len(), 1, "expected one state file");
    let state_content = fs::read_to_string(state_files[0].path()).unwrap();
    assert!(
        state_content.contains(".tar.gz"),
        "state project_path must point to archive, got: {state_content}"
    );
    assert!(
        !state_content.contains("myproject\""),
        "state must not retain the original directory path"
    );
}

#[test]
fn check_on_archived_project_succeeds() {
    let parent = tempdir().unwrap();
    let project = parent.path().join("myproject");
    fs::create_dir(&project).unwrap();
    make_old_file(&project);
    write_archive_config(&project);

    let state_dir = tempdir().unwrap();

    // Run once to archive the project.
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            project.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "initial run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!project.exists(), "project dir must be replaced by archive");

    let archive = find_archive(parent.path());

    // `frostx check` must succeed when given the archive path.
    let out = run_cmd(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            archive.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "check on archive failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_on_archived_project_executes_further_rules() {
    let parent = tempdir().unwrap();
    let project = parent.path().join("myproject");
    fs::create_dir(&project).unwrap();
    make_old_file(&project);

    let marker_path = parent.path().join("rule2_executed");
    let config = format!(
        r#"id = "a1b2c3d4-0000-0000-0000-000000000022"

[config.hook.mark]
command = "touch {marker}"
kind = "check"
run_on_archive = true

[[rule]]
after = "1h"
actions = ["archive.compress"]

[[rule]]
after = "1h"
actions = ["hook.mark"]
"#,
        marker = marker_path.display()
    );
    fs::write(project.join("frostx.toml"), &config).unwrap();

    let state_dir = tempdir().unwrap();

    // Run 1: archive the project (rule 1 fires).
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            project.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "run 1 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!project.exists(), "project dir must be replaced by archive");

    let archive = find_archive(parent.path());

    // Make the archive appear old so that rule 2's threshold is met.
    touch_old(&archive);

    // Run 2: operate on the archive path; rule 2 must fire.
    let out = run_cmd(
        &[
            "--yes",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "run",
            archive.to_str().unwrap(),
        ],
        parent.path(),
    );
    assert!(
        out.status.success(),
        "run 2 on archive failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        marker_path.exists(),
        "rule 2 hook must have created the marker file"
    );
}
