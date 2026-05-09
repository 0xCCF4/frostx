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

fn init_project(dir: &std::path::Path) {
    run(&["init", "."], dir);
}

#[test]
fn check_not_initialized_exits_3() {
    let tmp = tempdir().unwrap();
    let out = run(&["check", "."], tmp.path());
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn check_initialized_project_exits_0() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
    init_project(tmp.path());
    let state_dir = tempdir().unwrap();
    let out = run(
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
}

#[test]
fn check_json_output_shape() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("code.rs"), "// code").unwrap();
    init_project(tmp.path());
    let state_dir = tempdir().unwrap();
    let out = run(
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
    assert!(v["uuid"].is_string());
    assert!(v["inactive_seconds"].is_number());
    assert!(v["rules"].is_array());
}

#[test]
fn check_uuid_collision_exits_4() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();

    // Create and check project A so its path is recorded in state.
    let proj_a = tmp.path().join("proj_a");
    fs::create_dir(&proj_a).unwrap();
    fs::write(proj_a.join("code.rs"), "x").unwrap();
    run(&["init", "."], &proj_a);
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        &proj_a,
    );

    // Copy frostx.toml to proj_b (same UUID, different path).
    let proj_b = tmp.path().join("proj_b");
    fs::create_dir(&proj_b).unwrap();
    fs::copy(proj_a.join("frostx.toml"), proj_b.join("frostx.toml")).unwrap();
    fs::write(proj_b.join("code.rs"), "x").unwrap();

    let out = run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        &proj_b,
    );
    assert_eq!(
        out.status.code(),
        Some(4),
        "should detect UUID collision: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_shows_no_triggered_rules_on_fresh_project() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("fresh.txt"), "new").unwrap();
    init_project(tmp.path());
    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        tmp.path(),
    );
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let triggered: Vec<_> = v["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["triggered"].as_bool().unwrap_or(false))
        .collect();
    assert!(triggered.is_empty());
}
