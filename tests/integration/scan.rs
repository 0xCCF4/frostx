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
fn scan_finds_projects() {
    let root = tempdir().unwrap();
    let proj_a = root.path().join("proj_a");
    let proj_b = root.path().join("proj_b");
    fs::create_dir(&proj_a).unwrap();
    fs::create_dir(&proj_b).unwrap();

    run(&["init", "."], &proj_a);
    run(&["init", "."], &proj_b);

    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "scan",
            ".",
        ],
        root.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("invalid JSON");
    let arr = v.as_array().expect("expected array");
    assert_eq!(arr.len(), 2);
}

#[test]
fn scan_triggered_only_hides_fresh_projects() {
    let root = tempdir().unwrap();
    let proj = root.path().join("myproj");
    fs::create_dir(&proj).unwrap();
    fs::write(proj.join("code.rs"), "x").unwrap();
    run(&["init", "."], &proj);

    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "scan",
            "--triggered-only",
            ".",
        ],
        root.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[test]
fn scan_empty_root_returns_empty_array() {
    let root = tempdir().unwrap();
    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "scan",
            ".",
        ],
        root.path(),
    );
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}
