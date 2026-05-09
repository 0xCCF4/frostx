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
    "archive.tar_gz",
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

### `backup.verify`

Fails if the archive on the backup server cannot be read or its checksum does not match the locally recorded value.
Intended immediately after `backup.upload` to confirm the transfer succeeded.

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

Deletes common build artifact directories before archiving, reducing archive size. Shows the list of directories and
their sizes before asking for confirmation.

Default targets: `target/`, `node_modules/`, `.venv/`, `dist/`, `build/`, `.cache/`

```toml
[config.fs]
clean_artifacts = ["target/", "node_modules/"]  # override the default list
```

---

### `git.tag`

Creates an annotated git tag (`frostx-archive-<date>`) on the current HEAD to mark the last active state before the
project is archived.

No configuration required.

---

### `archive.tar_gz`

Creates a compressed archive of the project directory alongside the project folder, with a progress indicator.

Output: `<parent-dir>/<project-dir>-<uuid>-<date>.tar.gz`

```toml
[config.archive]
compression = "gz"   # gz (default) | zstd | xz
```

---

### `backup.upload`

Uploads the archive produced by `archive.tar_gz` to the configured backup server, with a progress indicator. Must follow
`archive.tar_gz` in the action chain.

```toml
[config.backup]
server = "rsync://backup.example.com/projects"  # required
```

---

### `hook`

Runs an arbitrary shell command in the project directory via `sh -c`. Stdout and stderr are captured and included in the
action message.

Exit code determines behavior:

- `0` - success, chain continues
- non-zero - failure, chain stops

This makes `hook` usable as both a **custom check** (assert a condition, exit non-zero if it fails) and a **custom
mutation** (perform an action, exit non-zero on error).

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
actions = ["hook.verify_backup", "hook.pre_archive", "archive.tar_gz"]
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
    "archive.tar_gz",
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
archive.tar_gz        <- create archive
backup.upload         <- store offsite
backup.verify         <- confirm transfer
local.delete          <- remove local copy
```

If you need git-specific behaviour (e.g. `git.clean`), use the `git.*` actions directly:

```
git.check_clean
git.check_pushed
git.clean             <- remove untracked files (git-only)
fs.clean_artifacts
git.tag
archive.tar_gz
backup.upload
backup.verify
local.delete
```
