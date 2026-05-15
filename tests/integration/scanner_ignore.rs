//! Integration tests for `.frostxignore` and hardcoded VCS directory exclusion.

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

fn write_config(dir: &std::path::Path, id: &str) {
    let config = format!(
        r#"id = "{id}"

[[rule]]
after = "90d"
actions = ["git.check_clean"]
"#
    );
    fs::write(dir.join("frostx.toml"), config).unwrap();
}

/// A project that contains only a `.git/` directory should not crash frostx;
/// the scanner must silently skip the VCS directory.
#[test]
fn check_succeeds_with_only_git_dir() {
    let dir = tempdir().unwrap();
    write_config(dir.path(), "a1b2c3d4-0000-0000-0000-000000000001");

    let git = dir.path().join(".git");
    fs::create_dir(&git).unwrap();
    fs::write(git.join("HEAD"), "ref: refs/heads/main").unwrap();
    fs::write(git.join("FETCH_HEAD"), "abc123 branch main").unwrap();

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A project with a `.frostxignore` file should not crash frostx, and the
/// ignore file itself must be parsed without error.
#[test]
fn check_succeeds_with_frostxignore_present() {
    let dir = tempdir().unwrap();
    write_config(dir.path(), "a1b2c3d4-0000-0000-0000-000000000002");

    fs::write(dir.path().join(".frostxignore"), "dist/\n*.log\n").unwrap();
    let dist = dir.path().join("dist");
    fs::create_dir(&dist).unwrap();
    fs::write(dist.join("bundle.js"), "compiled code").unwrap();
    fs::write(dir.path().join("debug.log"), "log output").unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Files inside `.jj/` must not be counted by the scanner; the command must
/// succeed without error.
#[test]
fn check_succeeds_with_only_jj_dir() {
    let dir = tempdir().unwrap();
    write_config(dir.path(), "a1b2c3d4-0000-0000-0000-000000000003");

    let jj = dir.path().join(".jj");
    fs::create_dir(&jj).unwrap();
    fs::write(jj.join("repo"), "jj state data").unwrap();

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--json",
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "check",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `frostx doctor` must validate a project that has a `.frostxignore` file
/// without reporting errors.
#[test]
fn doctor_succeeds_with_frostxignore() {
    let dir = tempdir().unwrap();
    write_config(dir.path(), "a1b2c3d4-0000-0000-0000-000000000004");
    fs::write(dir.path().join(".frostxignore"), "target/\n*.tmp\n").unwrap();

    let state_dir = tempdir().unwrap();
    let out = run_cmd(
        &[
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "doctor",
            dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "frostx doctor failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
