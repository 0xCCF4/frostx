# State Storage

frostx separates configuration from runtime state. `frostx.toml` is pure configuration and is never written to after
`frostx init` - it is safe to commit to version control.

## Location

Mutable state is stored under the [XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/) data
home:

```
$XDG_DATA_HOME/frostx/<uuid>.toml
```

`XDG_DATA_HOME` defaults to `~/.local/share/` when unset, giving:

```
~/.local/share/frostx/<uuid>.toml
```

The directory can be overridden at runtime with the `--state-dir` flag (applies to all commands):

```
frostx check --state-dir /mnt/shared/frostx-state ~/projects/my-app
```

The file is named after the project's UUID, so state survives the project folder being renamed or moved.

## State File Format

```toml
# last known absolute path - updated on every scan
project_path = "/home/user/projects/my-app"

# timestamp of the last frostx run against this project
last_scan = "2025-04-01T08:00:00Z"

# per-rule completion records (one entry per [[rule]] in frostx.toml)
[[rule]]
index = 1          # matches the 1-indexed rule in frostx.toml
completed = [# actions that have been successfully executed and need not repeat
    "archive.tar_gz",
    "backup.upload",
]
last_run = "2025-04-01T08:00:00Z"

[[rule]]
index = 2
completed = []
last_run = ""
```

## What Gets Recorded

Not all actions are recorded as completed:

| Action type                                                       | Recorded? | Reason                                                           |
|-------------------------------------------------------------------|-----------|------------------------------------------------------------------|
| **Checks** (`git.check_clean`, `backup.check`, ...)               | No        | Re-evaluated on every run; their result may change               |
| **Mutations** (`archive.tar_gz`, `backup.upload`, `local.delete`) | Yes       | One-time operations; re-running would be destructive or wasteful |

Use `frostx run --force` to re-execute completed mutation actions.

## UUID Collisions

A collision occurs when a project directory is copied - the copy inherits `frostx.toml` with the original's UUID, but
frostx's state file for that UUID records a different `project_path`.

**Detection:** on every command that loads state (`check`, `run`, `scan`), frostx compares the current working path
against the `project_path` in the state file. If they differ, frostx treats this as a collision.

**Behavior on collision:**

- The command is aborted with exit code `4`.
- An error is printed identifying the conflict:

```
error: UUID collision detected
  current path  /home/user/projects/my-app-copy
  state records /home/user/projects/my-app

This project appears to be a copy. Run `frostx init --force` to assign a new UUID
and start fresh state for this directory.
```

- The original project is unaffected; its state file is not modified.

**Resolution:** run `frostx init --force` in the copied directory. This generates a new UUID, writes it to
`frostx.toml`, and creates a clean state file. The copy is then treated as an independent project.

## Tracked-Project Registry

State files also serve as a registry of tracked projects. Use the `projects` subcommand to manage them explicitly:

```bash
frostx projects list               # list all tracked projects
frostx projects add ~/my-app       # register a project
frostx projects add --scan ~/src   # register every project found under ~/src
frostx projects rm ~/old-app       # unregister a project (deletes its state file)
```

`frostx projects check` and `frostx projects run` operate across all registered projects in one invocation.

## Stale State

If a project's `frostx.toml` is deleted or its UUID changes, the corresponding state file in `$XDG_DATA_HOME/frostx/`
becomes orphaned. Run `frostx gc` to find and remove them. Use `--dry-run` first to preview what would be deleted.
