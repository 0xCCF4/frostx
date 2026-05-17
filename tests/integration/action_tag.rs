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

/// A config with a defined `#offsite` override and a tagged action must pass doctor.
#[test]
fn tagged_backup_action_with_defined_override_is_valid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000001"

[config.backup]
server = "rsync://primary.example.com/"

[config.backup.overrides.offsite]
server = "rsync://offsite.example.com/"

[[rule]]
after = "90d"
actions = ["backup.check", "backup.upload#offsite"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A tagged action referencing an undefined override tag must fail doctor.
#[test]
fn tagged_backup_action_with_missing_override_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000002"

[config.backup]
server = "rsync://primary.example.com/"

[[rule]]
after = "90d"
actions = ["backup.upload#undefined"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("undefined"),
        "expected error mentioning tag name, got: {stdout}"
    );
}

/// A tagged action without any backup config at all should report the
/// standard "backup config missing" error (not a tag-specific error).
#[test]
fn tagged_backup_action_without_backup_config_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000003"

[[rule]]
after = "90d"
actions = ["backup.upload#offsite"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("backup"),
        "expected error mentioning backup, got: {stdout}"
    );
}

// ── archive.* tags ──────────────────────────────────────────────────────────

/// A defined `#tag` override on archive.compress must pass doctor.
#[test]
fn tagged_archive_action_with_defined_override_is_valid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000010"

[config.archive]
compression = "gz"

[config.archive.overrides.fast]
compression = "gz"

[[rule]]
after = "180d"
actions = ["archive.compress#fast"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An undefined `#tag` on archive.compress must fail doctor.
#[test]
fn tagged_archive_action_with_missing_override_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000011"

[config.archive]
compression = "gz"

[[rule]]
after = "180d"
actions = ["archive.compress#undefined"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("undefined"),
        "expected error mentioning tag name, got: {stdout}"
    );
}

/// A tag on archive.compress without [config.archive] at all must fail doctor.
#[test]
fn tagged_archive_without_config_section_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000012"

[[rule]]
after = "180d"
actions = ["archive.compress#fast"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── fs.* tags ───────────────────────────────────────────────────────────────

/// A defined `#tag` override on `fs.clean_artifacts` must pass doctor.
#[test]
fn tagged_fs_action_with_defined_override_is_valid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000020"

[config.fs]
extra_paths = []

[config.fs.overrides.minimal]
extra_paths = ["dist/"]

[[rule]]
after = "180d"
actions = ["fs.clean_artifacts#minimal"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An undefined `#tag` on `fs.clean_artifacts` must fail doctor.
#[test]
fn tagged_fs_action_with_missing_override_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000021"

[config.fs]
extra_paths = []

[[rule]]
after = "180d"
actions = ["fs.clean_artifacts#undefined"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── vcs.* tags ──────────────────────────────────────────────────────────────

/// A defined `#tag` override on `vcs.check_clean` must pass doctor.
#[test]
fn tagged_vcs_action_with_defined_override_is_valid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000030"

[config.vcs]
skip_if_no_vcs = false

[config.vcs.overrides.lenient]
skip_if_no_vcs = true

[[rule]]
after = "90d"
actions = ["vcs.check_clean#lenient"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An undefined `#tag` on `vcs.check_clean` must fail doctor.
#[test]
fn tagged_vcs_action_with_missing_override_is_invalid() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000031"

[config.vcs]
skip_if_no_vcs = false

[[rule]]
after = "90d"
actions = ["vcs.check_clean#undefined"]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An untagged backup action alongside a tagged one with a valid override
/// should use the base config without interference.
#[test]
fn untagged_and_tagged_actions_coexist() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("frostx.toml"),
        r#"
id = "a1b2c3d4-0000-0000-0000-000000000004"

[config.backup]
server = "rsync://primary.example.com/"

[config.backup.overrides.secondary]
server = "rsync://secondary.example.com/"

[[rule]]
after = "90d"
actions = [
    "backup.check",
    "backup.upload",
    "backup.check#secondary",
    "backup.upload#secondary",
]
"#,
    )
    .unwrap();
    let out = run(&["doctor", "."], tmp.path());
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
