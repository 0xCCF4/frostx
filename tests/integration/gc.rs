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
fn gc_detects_duplicate_path() {
    let tmp = tempdir().unwrap();
    let state_dir = tempdir().unwrap();

    let proj = tmp.path().join("myproject");
    fs::create_dir(&proj).unwrap();

    // Init and check to create state file A with project_path populated.
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

    // Extract UUID A from frostx.toml.
    let cfg_content = fs::read_to_string(proj.join("frostx.toml")).unwrap();
    let uuid_a_str = cfg_content
        .lines()
        .find(|l| l.starts_with("id = "))
        .and_then(|l| l.split('"').nth(1))
        .expect("id field not found in frostx.toml")
        .to_string();

    // Create UUID B as the new active owner for the same path.
    let uuid_b = uuid::Uuid::new_v4();
    let canonical_proj = proj.canonicalize().unwrap();

    // Write state file B pointing to the same project path.
    let state_b = format!("project_path = \"{}\"\n", canonical_proj.display());
    fs::write(state_dir.path().join(format!("{uuid_b}.toml")), state_b).unwrap();

    // Update frostx.toml to use UUID B, making state file A a stale duplicate.
    let new_cfg = cfg_content.replace(&uuid_a_str, &uuid_b.to_string());
    fs::write(proj.join("frostx.toml"), new_cfg).unwrap();

    // Dry-run: A detected as duplicate_path, nothing deleted.
    let out = run(
        &[
            "--dry-run",
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "gc",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let orphaned = v["orphaned"].as_array().unwrap();
    assert_eq!(orphaned.len(), 1);
    assert_eq!(orphaned[0]["reason"], "duplicate_path");
    assert_eq!(v["removed"], 0);
    assert_eq!(fs::read_dir(state_dir.path()).unwrap().count(), 2);

    // Without dry-run: stale file A deleted, active file B kept.
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "gc",
        ],
        tmp.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["removed"], 1);
    assert_eq!(fs::read_dir(state_dir.path()).unwrap().count(), 1);
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
