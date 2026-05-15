# Actions & Checks Reference

## Overview

Actions are the building blocks of a `[[rule]]`. They fall into two categories:

| Category     | Behavior                                                                  | Recorded in state?             |
|--------------|---------------------------------------------------------------------------|--------------------------------|
| **Check**    | Read-only assertion. Fails the chain if the condition is not met.         | No - re-evaluated on every run |
| **Mutation** | Performs a change. Skipped on subsequent runs unless `--force` is passed. | Yes                            |

Within a rule, actions execute in declaration order. The first failure stops the chain.

frostx is an interactive tool - before any destructive mutation executes, it prints a summary of what is about to happen
and asks for confirmation. Pass `--yes` to skip confirmations (e.g. in scripts).

---

## Actions after `archive.compress`

`archive.compress` **replaces** the project directory with a single archive file. Every action that runs after it in
the same rule — including on subsequent re-runs — receives the archive file path as `project_path` rather than a
directory.

Most actions are designed to operate on a live project directory (VCS operations, filesystem cleanups, etc.) and cannot
meaningfully run against a compressed archive. frostx enforces this automatically:

| Action kind | Compressed-archive compatible? | Behavior                                        |
|-------------|--------------------------------|-------------------------------------------------|
| Check       | No                             | **Skipped** — chain continues                   |
| Mutation    | No                             | **Failed** — chain stops                        |
| Check       | Yes                            | Runs normally                                   |
| Mutation    | Yes (already completed)        | **Completed** — skipped as in any other re-run  |
| Mutation    | Yes (not yet completed)        | Runs normally                                   |

Checks are skipped rather than failed because they are re-evaluated gates: if the project has already been archived the
condition they assert (e.g., "working tree is clean") no longer applies. Mutations are failed because a mutation that
cannot act on an archive represents a genuine pipeline configuration error.

### Which actions are archive-compatible

| Action            | Compatible? | Notes                                                             |
|-------------------|-------------|-------------------------------------------------------------------|
| `vcs.check_clean` | No          | Skipped when project is an archive                                |
| `vcs.check_pushed`| No          | Skipped when project is an archive                                |
| `vcs.mark`        | No          | Fails when project is an archive                                  |
| `git.check_clean` | No          | Skipped when project is an archive                                |
| `git.check_pushed`| No          | Skipped when project is an archive                                |
| `git.clean`       | No          | Fails when project is an archive                                  |
| `git.tag`         | No          | Fails when project is an archive                                  |
| `jj.check_clean`  | No          | Skipped when project is an archive                                |
| `jj.check_pushed` | No          | Skipped when project is an archive                                |
| `jj.bookmark`     | No          | Fails when project is an archive                                  |
| `fs.clean_artifacts` | No       | Fails when project is an archive                                  |
| `archive.compress`| No          | Always a completed mutation by the time this matters              |
| `backup.check`    | **Yes**     | Queries backup server — no local filesystem access                |
| `backup.upload`   | **Yes**     | Uploads the archive file                                          |
| `backup.verify`   | **Yes**     | Verifies upload — reads the local archive for checksum comparison |
| `local.delete`    | **Yes**     | Deletes the archive file                                          |
| `hook.*`          | Opt-in      | Set `run_on_archive = true` in `[config.hook.<name>]`; default is not compatible         |
| `notify.*`        | **Yes**     | Displays a message — no filesystem access                         |

### Re-runs after a partial pipeline

Because VCS check actions are skipped (not failed) when the project is an archive, a pipeline that was interrupted
mid-way through — say `backup.upload` failed on the first run — will resume cleanly on the next run:

```
Run 1: git.check_clean ✓  archive.compress ✓  backup.upload ✗  (network error)
Run 2: git.check_clean ↷  archive.compress ✓  backup.upload ✓  backup.verify ✓
                      ↑
                      Skipped — project is now an archive
```

The mutation `archive.compress` is shown as `Completed` because it was recorded in state on run 1. The check
`git.check_clean` is skipped because the project is a compressed archive. The pipeline proceeds to `backup.upload`
and completes.

## Configuration

Actions that require parameters are configured in a top-level `[config.<category>]` section of `frostx.toml`, separate
from the rule definitions.

```toml
[config.backup]
server = "rsync://backup.example.com/projects"

[config.hook.pre_archive]
command = "make clean"

[[rule]]
after = "180d"
actions = [
    "git.check_clean",
    "hook.pre_archive",
    "archive.compress",
    "backup.upload",
    "backup.verify",
    "local.delete",
]
```

---

## VCS-agnostic actions

The `vcs.*` actions auto-detect the VCS in use and delegate to the appropriate backend. They are the recommended choice
when you want your `frostx.toml` to work with any supported VCS.

Supported backends (detection order):

| Backend      | Detected by       |
|--------------|-------------------|
| Jujutsu (jj) | `.jj/` directory  |
| git          | `.git/` directory |

When both `.jj` and `.git` are present (jj with a git backend), jj takes precedence.

If no supported VCS is detected, all `vcs.*` actions **fail** by default. To skip silently
instead, set `skip_if_no_vcs = true` in `[config.vcs]`:

```toml
[config.vcs]
skip_if_no_vcs = true
```

---

### `vcs.check_clean`

Delegates to `jj.check_clean` or `git.check_clean` based on the detected VCS.

Fails if no VCS is detected (unless `skip_if_no_vcs = true`).

---

### `vcs.check_pushed`

Delegates to `jj.check_pushed` or `git.check_pushed` based on the detected VCS.

Fails if no VCS is detected (unless `skip_if_no_vcs = true`).

---

### `vcs.mark` *(mutation)*

Delegates to `jj.bookmark` or `git.tag` based on the detected VCS.

Fails if no VCS is detected (unless `skip_if_no_vcs = true`).

---

## Checks

### `git.check_clean`

Fails if the project directory is a git repository with uncommitted changes (staged or unstaged). Prints the output of
`git status` so the user can see exactly what is unclean.

No configuration required.

---

### `git.check_pushed`

Fails if the repository has commits not yet pushed to any configured remote. Automatically runs `git fetch --all` first
to ensure the comparison is against the current remote state, not a stale local ref.

No configuration required.

---

### `backup.check`

Fails if no archive for this project UUID is found on the configured backup server.

```toml
[config.backup]
server = "rsync://backup.example.com/projects"  # required
```

---

## Mutations

### `git.clean`

Removes untracked files and directories from the repository (`git clean -fd`). Before executing, shows the full list of
files that would be removed (equivalent to `git clean -nfd`) and asks for confirmation.

Complements `fs.clean_artifacts`: `git.clean` removes anything untracked by git; `fs.clean_artifacts` removes known
build directories regardless of git tracking status. For most projects, one or the other is sufficient.

No configuration required.

---

### `fs.clean_artifacts`

Deletes build artifact directories before archiving, reducing archive size. Shows the list of directories and their
sizes before asking for confirmation.

Each built-in **cleaner** checks for a language-specific marker file before touching anything — so a `target/`
directory is only removed when a `Cargo.toml` is also present.

| Cleaner  | Marker file(s)                    | Removes          |
|----------|-----------------------------------|------------------|
| `rust`   | `Cargo.toml`                      | `target/`        |
| `node`   | `package.json`                    | `node_modules/`  |
| `python` | `pyproject.toml` or `setup.py`    | `.venv/`         |

All cleaners are enabled by default. Use `[config.fs.cleaners]` to disable individual cleaners, and `extra_paths` for
paths that should always be removed regardless of marker files.

```toml
[config.fs]
extra_paths = ["dist/", "build/"]  # always removed if they exist

[config.fs.cleaners]
python = false  # skip Python detection entirely
```

---

### `git.tag`

Creates an annotated git tag (`frostx-archive-<date>`) on the current HEAD to mark the last active state before the
project is archived.

No configuration required.

---

### `archive.compress`

Creates a compressed archive of the project directory and **replaces** the project directory with it. The original
directory is removed after the archive is written successfully.

After this action completes, all subsequent actions in the pipeline receive the archive file path as their
`project_path`. Actions that cannot operate on a compressed archive are skipped (checks) or failed (mutations). See
[Actions after `archive.compress`](#actions-after-archivecompress) for details.

Output: `<parent-dir>/<project-dir>-<uuid>-<date>.tar.gz`

```toml
[config.archive]
compression = "gz"   # gz (default) | zstd | xz
```

---

### `backup.upload`

Uploads the project to the configured backup server, with a progress indicator.

```toml
[config.backup]
server = "rsync://backup.example.com/projects"  # required
```

---

### `backup.verify`

Confirms that the remote archive matches the local copy by checksum. Intended immediately after `backup.upload`.

The verification strategy depends on the server URL scheme:

- **`rsync://`** — uses `rsync --checksum --dry-run --itemize-changes` to compare the remote file against the local
  archive without downloading it.
- **`ssh://`** — runs `sha256sum` on the remote host via SSH and compares against a locally-computed SHA-256. Falls
  back to downloading the archive and comparing checksums locally if the remote command is unavailable.

`backup.verify` is a **mutation**: once the integrity of a specific upload is confirmed, frostx records the result
and skips re-verification on subsequent runs (pass `--force` to re-verify).

```toml
[config.backup]
server = "rsync://backup.example.com/projects"  # required
```

---

### `hook`

Runs an arbitrary shell command via `sh -c`. Stdout and stderr are captured and included in the action message.

Exit code determines behavior:

- `0` - success, chain continues
- non-zero - failure, chain stops

This makes `hook` usable as both a **custom check** (assert a condition, exit non-zero if it fails) and a **custom
mutation** (perform an action, exit non-zero on error).

By default, hooks do **not** run when the project is a compressed archive. Set `run_on_archive = true` to allow a
hook to run after `archive.compress`:

```toml
[config.hook.post_archive]
command = "my-notifier --archive $FROSTX_PROJECT_PATH"
kind = "mutation"
run_on_archive = true
```

When `run_on_archive = true` and the project is a compressed archive:
- The command's working directory is set to the archive's **parent directory** (not the archive file itself, which is not a valid CWD).
- `$FROSTX_PROJECT_PATH` holds the full path to the archive file.
- `$FROSTX_ARCHIVE` is set to `1` (it is `0` when running against an uncompressed directory).

When `run_on_archive = false` (the default), the hook follows the same skip/fail logic as other non-compatible
actions: skipped if it is a check, failed if it is a mutation.

```toml
[config.hook.<name>]
command = "my-backup-tool verify"
kind = "check"    # check | mutation (default: mutation)
# "check" hooks are never recorded as completed and re-run every time
```

Referenced in a rule as `hook.<name>`:

```toml
[[rule]]
after = "90d"
actions = ["hook.verify_backup", "hook.pre_archive", "archive.compress"]
```

---

### `local.delete`

Deletes the local project directory. Always requires explicit confirmation regardless of `--yes`, displaying the full
path and size before proceeding.

No configuration required.

---

### `notify.<name>`

Displays a configurable message and pauses the pipeline until the user explicitly confirms. Useful for inserting a
manual review checkpoint before destructive steps.

Always requires explicit confirmation regardless of `--yes`.

If the user declines, the action is skipped and the chain stops.

```toml
[config.notify.review_checklist]
message = "Review the pre-archive checklist at https://wiki/checklist before continuing."

[[rule]]
after = "180d"
actions = [
    "git.check_clean",
    "notify.review_checklist", # pause and confirm before archiving
    "archive.compress",
    "backup.upload",
    "local.delete",
]
```

Multiple named notifications can be defined independently:

```toml
[config.notify.pre_archive]
message = "Verify the project is ready to archive."

[config.notify.post_backup]
message = "Confirm the backup was recorded in the project log."
```

---

## Recommended Pipeline Order

For a full archive-and-delete workflow (VCS-agnostic):

```
vcs.check_clean       <- ensure no uncommitted changes
vcs.check_pushed      <- auto-fetches before checking
fs.clean_artifacts    <- remove known build dirs
vcs.mark              <- tag / bookmark last active state
archive.compress        <- create archive *and* remove project dir
backup.upload         <- store offsite (receives archive file path)
backup.verify         <- confirm transfer
```

If you need git-specific behaviour (e.g. `git.clean`), use the `git.*` actions directly:

```
git.check_clean
git.check_pushed
git.clean             <- remove untracked files (git-only)
fs.clean_artifacts
git.tag
archive.compress        <- create archive *and* remove project dir
backup.upload
backup.verify
```
