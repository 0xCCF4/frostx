use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn frostx_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps
    p.pop(); // debug
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
fn init_creates_config() {
    let tmp = tempdir().unwrap();
    let out = run(&["init", "."], tmp.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(tmp.path().join("frostx.toml").exists());
}

#[test]
fn init_assigns_uuid() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    let content = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    assert!(content.contains("id = \""));
}

#[test]
fn init_twice_without_force_fails() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    let out = run(&["init", "."], tmp.path());
    assert!(!out.status.success());
}

#[test]
fn init_force_changes_uuid() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    let content1 = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    run(&["init", "--force", "."], tmp.path());
    let content2 = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    // UUID should differ after --force.
    assert_ne!(content1, content2);
}

#[test]
fn init_force_preserves_existing_config() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    // Manually add a custom rule to simulate a real project.
    let config_path = tmp.path().join("frostx.toml");
    let mut content = fs::read_to_string(&config_path).unwrap();
    content.push_str("\n[[rule]]\nafter = \"30d\"\nactions = [\"vcs.check_clean\"]\n");
    fs::write(&config_path, &content).unwrap();

    let uuid_before = content
        .lines()
        .find(|l| l.starts_with("id = "))
        .unwrap()
        .to_string();

    run(&["init", "--force", "."], tmp.path());

    let content_after = fs::read_to_string(&config_path).unwrap();
    let uuid_after = content_after
        .lines()
        .find(|l| l.starts_with("id = "))
        .unwrap()
        .to_string();
    assert_ne!(
        uuid_before, uuid_after,
        "UUID should be replaced by --force"
    );
    assert!(
        content_after.contains("vcs.check_clean"),
        "existing rule should be preserved"
    );
}

#[test]
fn init_force_applies_include_override() {
    let tmp = tempdir().unwrap();
    run(&["init", "."], tmp.path());
    // Manually add a custom rule to verify it survives the --include override.
    let config_path = tmp.path().join("frostx.toml");
    let mut content = fs::read_to_string(&config_path).unwrap();
    content.push_str("\n[[rule]]\nafter = \"60d\"\nactions = [\"vcs.check_clean\"]\n");
    fs::write(&config_path, &content).unwrap();

    run(
        &["init", "--force", "--include", "new-template", "."],
        tmp.path(),
    );

    let content_after = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    assert!(
        content_after.contains("new-template"),
        "--include flag should update the include list"
    );
    assert!(
        content_after.contains("vcs.check_clean"),
        "existing rule should still be present after --include override"
    );
}

#[test]
fn init_json_output() {
    let tmp = tempdir().unwrap();
    let out = run(&["--json", "init", "."], tmp.path());
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON");
    assert!(v["uuid"].is_string());
    assert!(v["path"].is_string());
}

#[test]
fn init_with_include_writes_include() {
    let tmp = tempdir().unwrap();
    let out = run(&["init", "--include", "my-template", "."], tmp.path());
    assert!(out.status.success());
    let content = fs::read_to_string(tmp.path().join("frostx.toml")).unwrap();
    assert!(content.contains("my-template"));
}

#[test]
fn check_with_relative_include_resolves_correctly() {
    let tmp = tempdir().unwrap();
    // Write a shared fragment relative to the project dir.
    fs::write(
        tmp.path().join("shared.toml"),
        "[[rule]]\nafter = \"999d\"\nactions = [\"git.check_clean\"]\n",
    )
    .unwrap();
    // Init with a relative include.
    run(&["init", "."], tmp.path());
    // Patch frostx.toml to reference the relative include.
    let config_path = tmp.path().join("frostx.toml");
    let content = fs::read_to_string(&config_path).unwrap();
    // Replace the default empty include list with our relative path.
    let content = content.replace("include = []", "include = [\"./shared.toml\"]");
    fs::write(&config_path, &content).unwrap();

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
        "relative include should resolve against project dir, not cwd: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
