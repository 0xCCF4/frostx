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

#[test]
fn gc_empty_state_dir_exits_0() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();
    let out = run(
        &["--state-dir", state_dir.path().to_str().unwrap(), "gc"],
        tmp.path(),
    );
    assert!(out.status.success());
}

#[test]
fn gc_dry_run_does_not_delete() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();

    // Init a project to create a state file, then remove the project.
    let proj = tmp.path().join("ghost");
    fs::create_dir(&proj).unwrap();
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "init",
            ".",
        ],
        &proj,
    );
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        &proj,
    );
    // Remove the project directory so it becomes orphaned.
    fs::remove_dir_all(&proj).unwrap();

    // Count state files before.
    let before: usize = fs::read_dir(state_dir.path()).unwrap().count();

    let out = run(
        &[
            "--dry-run",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "gc",
        ],
        tmp.path(),
    );
    assert!(out.status.success());

    // State files must still be there.
    let after: usize = fs::read_dir(state_dir.path()).unwrap().count();
    assert_eq!(before, after);
}

#[test]
fn gc_removes_orphaned_state() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();

    let proj = tmp.path().join("gone");
    fs::create_dir(&proj).unwrap();
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "init",
            ".",
        ],
        &proj,
    );
    run(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            ".",
        ],
        &proj,
    );
    fs::remove_dir_all(&proj).unwrap();

    let out = run(
        &["--state-dir", state_dir.path().to_str().unwrap(), "gc"],
        tmp.path(),
    );
    assert!(out.status.success());
    assert_eq!(fs::read_dir(state_dir.path()).unwrap().count(), 0);
}

#[test]
fn gc_json_shape() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "gc",
        ],
        tmp.path(),
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("invalid JSON");
    assert!(v["orphaned"].is_array());
    assert!(v["removed"].is_number());
}
