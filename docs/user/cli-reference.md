# frostx CLI Reference

## Synopsis

```
frostx <COMMAND> [OPTIONS] [PATH]
```

`PATH` defaults to the current working directory when omitted.

## Global Options

| Flag                | Short | Description                                                                   |
|---------------------|-------|-------------------------------------------------------------------------------|
| `--dry-run`         | `-n`  | Show what would happen without executing any actions                          |
| `--json`            |       | Output all results as JSON instead of human-readable text                     |
| `--verbose`         | `-v`  | Increase output verbosity (repeat for more: `-vv`)                            |
| `--quiet`           | `-q`  | Suppress all output except errors                                             |
| `--yes`             | `-y`  | Skip interactive confirmations (except `local.delete`, which always confirms) |
| `--config <FILE>`   |       | Use a specific `frostx.toml` instead of the one in PATH                       |
| `--library <DIR>`   |       | Override the config library directory (default: `~/.config/frostx/library/`)  |
| `--state-dir <DIR>` |       | Override the state directory (default: `$XDG_DATA_HOME/frostx/`)              |

`--json` and `--quiet` are mutually exclusive. `--json` implies machine-readable output on stdout; errors are still
written to stderr as JSON.

---

## Output Formats

### Human-readable (default)

Colored, aligned text output intended for interactive use. Respects `NO_COLOR`.

### JSON (`--json`)

A single JSON object (or array for `scan`) written to stdout. The schema is stable and versioned via a top-level
`"frostx_version"` field. All timestamps are ISO 8601. All durations are in seconds.

Errors and warnings are written to stderr as a JSON object:

```json
{
  "frostx_version": "0.1.0",
  "error": "frostx.toml not found",
  "code": 3
}
```

---

## Commands

### `init`

Initialize a new frostx project in a directory by creating a `frostx.toml` with a generated UUID and a starter
configuration.

```
frostx init [OPTIONS] [PATH]
```

**Options:**

| Flag               | Description                                         |
|--------------------|-----------------------------------------------------|
| `--include <NAME>` | Bootstrap from a library template (can be repeated) |
| `--force`          | Overwrite an existing `frostx.toml`                 |

**Behavior:**

- Generates a UUID v4 and writes it as `id` in the new `frostx.toml`.
- If `--include` is given, the named library entries are added to the `include` list.
- Exits with a non-zero code if `frostx.toml` already exists and `--force` is not set.
- When `--force` is used on a directory with an existing UUID, a new UUID is generated and a fresh state file is
  created. Use this to resolve a UUID collision after copying a project (
  see [State: UUID Collisions](state.md#uuid-collisions)).

**Human-readable output:**

```
initialized  ~/projects/my-app
uuid         a1b2c3d4-e5f6-...
```

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "path": "/home/user/projects/my-app",
  "uuid": "a1b2c3d4-e5f6-..."
}
```

---

### `check`

Inspect a project without executing any actions. Reports inactivity duration, which rules would trigger, and the
expected next actions.

```
frostx check [OPTIONS] [PATH]
```

**Human-readable output:**

```
project  my-app  (a1b2c3d4-...)
path     ~/projects/my-app
inactive 97 days

rule #1  after=90d  ● TRIGGERED
  ✓ git.check_clean    - working tree is clean
  ✓ git.check_pushed   - all commits pushed
  ✗ backup.check       - not found on backup server

rule #2  after=180d  ○ not yet (83 days remaining)
```

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "project": "my-app",
  "path": "/home/user/projects/my-app",
  "uuid": "a1b2c3d4-...",
  "inactive_seconds": 8380800,
  "rules": [
    {
      "index": 1,
      "after_seconds": 7776000,
      "triggered": true,
      "actions": [
        {
          "name": "git.check_clean",
          "status": "ok",
          "message": "working tree is clean"
        },
        {
          "name": "git.check_pushed",
          "status": "ok",
          "message": "all commits pushed"
        },
        {
          "name": "backup.check",
          "status": "failed",
          "message": "not found on backup server"
        }
      ]
    },
    {
      "index": 2,
      "after_seconds": 15552000,
      "triggered": false,
      "remaining_seconds": 7171200,
      "actions": []
    }
  ]
}
```

---

### `run`

Execute the inactivity pipeline for a project. Evaluates each rule in order and runs triggered actions sequentially,
stopping a chain on the first failure.

```
frostx run [OPTIONS] [PATH]
```

**Options:**

| Flag              | Description                                               |
|-------------------|-----------------------------------------------------------|
| `--rule <N>`      | Run only rule number N (1-indexed)                        |
| `--action <NAME>` | Run a single named action, skipping rule threshold checks |
| `--force`         | Re-execute completed mutation actions                     |

**Behavior:**

- A rule is skipped if the inactivity threshold has not been reached.
- Within a rule, if an action fails, subsequent actions in that rule are skipped.
- Bail out: a failure in rule #1 does prevents rule #2 from being evaluated.
- Completed mutation actions are recorded in the state file and skipped on subsequent runs unless `--force` is given.

**JSON output** - one JSON object per action, streamed as newline-delimited JSON (NDJSON):

```json
{
  "frostx_version": "0.1.0",
  "rule": 1,
  "action": "git.check_clean",
  "status": "ok",
  "message": "working tree is clean"
}
{
  "frostx_version": "0.1.0",
  "rule": 1,
  "action": "git.check_pushed",
  "status": "ok",
  "message": "all commits pushed"
}
{
  "frostx_version": "0.1.0",
  "rule": 1,
  "action": "backup.check",
  "status": "failed",
  "message": "not found on backup server"
}
```

`run` uses NDJSON rather than a single object so that progress is visible in real time when piped.

---

### `scan`

Recursively walk a directory tree and report the status of every frostx project found.

```
frostx scan [OPTIONS] [ROOT]
```

**Options:**

| Flag               | Description                                            |
|--------------------|--------------------------------------------------------|
| `--triggered-only` | Only show projects with at least one triggered rule    |
| `--depth <N>`      | Maximum directory depth to search (default: unlimited) |

**JSON output** - a JSON array of `check`-shaped objects:

```json
[
  {
    "frostx_version": "0.1.0",
    "project": "my-app",
    "path": "...",
    "uuid": "...",
    "inactive_seconds": 8380800,
    "rules": [
      ...
    ]
  },
  {
    "frostx_version": "0.1.0",
    "project": "other",
    "path": "...",
    "uuid": "...",
    "inactive_seconds": 1234567,
    "rules": [
      ...
    ]
  }
]
```

---

### `doctor`

Validate a `frostx.toml` file for correctness: checks UUID presence, valid durations, resolvable includes, and known
action names.

```
frostx doctor [PATH]
```

Exits `0` if valid, `1` if errors, `2` if only warnings.

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "valid": false,
  "errors": [
    {
      "field": "rule[1].after",
      "message": "invalid duration '90x'"
    }
  ],
  "warnings": [
    {
      "field": "include[0]",
      "message": "library entry 'unknown' not found"
    }
  ]
}
```

---

### `gc`

Remove orphaned state files - entries in the state directory whose recorded `project_path` no longer contains a
`frostx.toml` with a matching UUID.

```
frostx gc [OPTIONS]
```

**Behavior:**

- Reads every `<uuid>.toml` in the state directory.
- Checks whether the recorded `project_path` exists and its `frostx.toml` contains the same UUID.
- Deletes any state file that fails this check, unless `--dry-run` is set.

**Human-readable output:**

```
orphaned  a1b2c3d4-....toml  (path no longer exists: /home/user/projects/old-app)
orphaned  e5f6a7b8-....toml  (UUID mismatch at /home/user/projects/other-app)

2 orphaned state files found. Run without --dry-run to delete.
```

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "orphaned": [
    {
      "state_file": "a1b2c3d4-....toml",
      "reason": "path_missing",
      "path": "/home/user/projects/old-app"
    },
    {
      "state_file": "e5f6a7b8-....toml",
      "reason": "uuid_mismatch",
      "path": "/home/user/projects/other-app"
    }
  ],
  "removed": 0
}
```

---

### `projects`

Manage the tracked-project registry. Every `frostx check` or `frostx run` call registers the project automatically;
these sub-commands let you manage that registry explicitly.

#### `projects list`

List all currently tracked projects.

```
frostx projects list
```

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "projects": [
    {
      "uuid": "a1b2c3d4-...",
      "path": "/home/user/projects/my-app",
      "last_scan": "2026-01-15T10:30:00Z"
    },
    {
      "uuid": "e5f6a7b8-...",
      "path": "/home/user/projects/other",
      "last_scan": null
    }
  ]
}
```

---

#### `projects add`

Register one or more projects in the state directory without running `check` first.

```
frostx projects add [--scan <DIR>] [PATH...]
```

**Options:**

| Flag           | Description                                                  |
|----------------|--------------------------------------------------------------|
| `--scan <DIR>` | Recursively walk DIR and register every frostx project found |

**Behavior:**

- Reads the UUID from `frostx.toml` at each PATH.
- Aborts a single path with an error if a UUID collision is detected; continues with the remaining paths.
- Paths where `frostx.toml` is absent are reported in `skipped`.

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "added": [
    {
      "uuid": "a1b2c3d4-...",
      "path": "/home/user/projects/my-app"
    }
  ],
  "skipped": [
    {
      "path": "/tmp/not-a-project",
      "reason": "no frostx.toml found in ..."
    }
  ]
}
```

---

#### `projects rm`

Unregister a project by deleting its state file.

```
frostx projects rm <PATH>
```

**Behavior:**

- Reads the UUID from `frostx.toml` at PATH if it exists; otherwise searches state files by recorded path.
- Deletes the matching state file.
- Exits `3` if no matching state entry is found.

**JSON output:**

```json
{
  "frostx_version": "0.1.0",
  "uuid": "a1b2c3d4-...",
  "path": "/home/user/projects/my-app"
}
```

---

#### `projects check`

Run `check` on every tracked project.

```
frostx projects check
```

Human mode prints each project's check output in sequence. JSON mode outputs a single array of check objects (same shape
as `scan`).

**JSON output** - array of `check`-shaped objects:

```json
[
  {
    "frostx_version": "0.1.0",
    "project": "my-app",
    "path": "...",
    "uuid": "...",
    "inactive_seconds": 8380800,
    "rules": [
      ...
    ]
  },
  {
    "frostx_version": "0.1.0",
    "project": "other",
    "path": "...",
    "uuid": "...",
    "inactive_seconds": 1234567,
    "rules": [
      ...
    ]
  }
]
```

---

#### `projects run`

Run the inactivity pipeline for every tracked project.

```
frostx projects run [OPTIONS]
```

**Options:**

| Flag              | Description                                                           |
|-------------------|-----------------------------------------------------------------------|
| `--rule <N>`      | Run only rule number N (1-indexed) on every project                   |
| `--action <NAME>` | Run a single named action on every project, skipping threshold checks |
| `--force`         | Re-execute completed mutation actions                                 |

**JSON output** - NDJSON per action, same as `run --json` but with an added `"project"` field:

```json
{
  "frostx_version": "0.1.0",
  "project": "/home/user/projects/my-app",
  "rule": 1,
  "action": "git.check_clean",
  "status": "ok",
  "message": "working tree is clean"
}
{
  "frostx_version": "0.1.0",
  "project": "/home/user/projects/other",
  "rule": 1,
  "action": "git.check_clean",
  "status": "failed",
  "message": "uncommitted changes"
}
```

---

## Exit Codes

| Code | Meaning                                                                 |
|------|-------------------------------------------------------------------------|
| `0`  | Success                                                                 |
| `1`  | General error (invalid config, action failure, etc.)                    |
| `2`  | Warning (used by `doctor`: valid config but with warnings)              |
| `3`  | Project not initialized (no `frostx.toml` found)                        |
| `4`  | UUID collision detected (project is a copy - run `frostx init --force`) |

---

## Environment Variables

| Variable        | Description                                                                                       |
|-----------------|---------------------------------------------------------------------------------------------------|
| `XDG_DATA_HOME` | Base directory for state files; frostx uses `$XDG_DATA_HOME/frostx/` (default: `~/.local/share/`) |
| `NO_COLOR`      | Disable colored output                                                                            |
