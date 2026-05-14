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
fn template_substitution_in_include_file() {
    let tmp = tempdir().unwrap();
    let lib_dir = tempdir().unwrap();

    // Write a library template that uses a placeholder.
    fs::write(
        lib_dir.path().join("my-backup.toml"),
        "[config.backup]\nserver = \"{{backup_server}}\"\n",
    )
    .unwrap();

    // Init with --yes to skip the questionnaire (non-interactive).
    let out = run(
        &[
            "--library",
            lib_dir.path().to_str().unwrap(),
            "--yes",
            "init",
            "--include",
            "my-backup",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Patch frostx.toml to add the [template] section.
    let config_path = tmp.path().join("frostx.toml");
    let content = fs::read_to_string(&config_path).unwrap();
    let patched = content + "\n[template]\nbackup_server = \"rsync://test.example.com/\"\n";
    fs::write(&config_path, &patched).unwrap();

    // doctor should succeed (backup config is resolved from template).
    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--library",
            lib_dir.path().to_str().unwrap(),
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "doctor",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        out.status.success(),
        "doctor should pass with template resolved: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unresolved_template_var_fails_at_load() {
    let tmp = tempdir().unwrap();
    let lib_dir = tempdir().unwrap();

    fs::write(
        lib_dir.path().join("needs-var.toml"),
        "[config.backup]\nserver = \"{{missing_var}}\"\n",
    )
    .unwrap();

    run(
        &[
            "--library",
            lib_dir.path().to_str().unwrap(),
            "--yes",
            "init",
            "--include",
            "needs-var",
            ".",
        ],
        tmp.path(),
    );

    // doctor should fail because {{missing_var}} is not in [template].
    let state_dir = tempdir().unwrap();
    let out = run(
        &[
            "--library",
            lib_dir.path().to_str().unwrap(),
            "--state-dir",
            state_dir.path().to_str().unwrap(),
            "doctor",
            ".",
        ],
        tmp.path(),
    );
    assert!(
        !out.status.success(),
        "should fail when template variable is unresolved"
    );
}
