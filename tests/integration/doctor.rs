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
fn doctor_valid_config_exits_0() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn doctor_invalid_config_exits_1() {
    let tmp = tempdir().unwrap();
    // Write a broken config (nil UUID).
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "00000000-0000-0000-0000-000000000000"
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn doctor_missing_project_exits_3() {
    let tmp = tempdir().unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn doctor_warnings_exits_2() {
    let tmp = tempdir().unwrap();
    // A rule with an empty actions list generates a warning.
    fs::write(
        tmp.path().join("frostx.toml"),
        "id = \"a1b2c3d4-0000-0000-0000-000000000099\"\n\n[[rule]]\nafter = \"90d\"\nactions = []\n",
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(2),
        "should exit 2 for warnings-only: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn doctor_json_shape() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    let out = run(&["--json", "doctor", "."], tmp.path());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("invalid JSON");
    assert!(v["valid"].is_boolean());
    assert!(v["errors"].is_array());
    assert!(v["warnings"].is_array());
}
