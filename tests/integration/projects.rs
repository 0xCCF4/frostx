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
