# frostx

**frostx** manages the lifecycle of project directories based on filesystem inactivity. Place a `frostx.toml` in any
folder, configure inactivity thresholds and actions, and frostx will take care of your stale projects;
execute your preconfigured pipelines to - checking git state, archiving, uploading backups, or deleting local copies -
when a project becomes inactive.

## What problem does it solve?

Projects often accumulate as folders on disk, and it's easy to forget about them until you finally realize you have 200
"old" projects taking up space in your `Documents` folder. But you feel like cleaning them up is a chore; you don't want
to manually check each one for uncommitted changes, create archives, upload to back up storage, and delete the local
copy. Why not configuring a pipeline when you first create the project, and let it automatically archive and clean up
after a period of inactivity? That's what frostx does. Just configure frostx to start once in a while when you
open your first shell of the day, and it will take care of the rest.

## Quick start

```bash
# Initialize a project
frostx init ~/projects/my-app

# See inactivity status and whether any rules are triggered
frostx check ~/projects/my-app

# Execute the pipeline (dry-run first)
frostx run --dry-run ~/projects/my-app
frostx run ~/projects/my-app

# Validate frostx.toml
frostx doctor ~/projects/my-app
```

## frostx.toml example

```toml
id = "auto-assigned-uuid"   # assigned by frostx init, never change this, unique project identifier

[config.backup]
server = "rsync://backup.example.com/projects"

[config.notify.pre_archive]
message = "Review the project one last time before it is archived."

[[rule]]
after = "90d"
actions = [
    "git.check_clean", # fail if uncommitted changes exist
    "git.check_pushed", # fail if unpushed commits exist
    "backup.check", # fail if not on backup server
]

[[rule]]
after = "180d"
actions = [
    "notify.pre_archive", # pause and ask for confirmation
    "fs.clean_artifacts", # remove build dirs (target/, node_modules/, ...)
    "git.tag", # tag HEAD as last active state
    "archive.compress", # create compressed archive
    "backup.upload", # upload to backup server
    "backup.verify", # confirm upload integrity
    "local.delete", # remove local copy (always confirms interactively)
]
```

Full schema reference: [docs/user/frostx-toml.md](docs/user/frostx-toml.md)

## Actions

| Action               | Kind         | Description                                       |
|----------------------|--------------|---------------------------------------------------|
| `git.check_clean`    | check        | Fail if uncommitted changes exist                 |
| `git.check_pushed`   | check        | Fail if unpushed commits exist                    |
| `backup.check`       | check        | Fail if no backup exists on the server            |
| `backup.verify`      | check        | Fail if the backup archive checksum doesn't match |
| `git.clean`          | mutation     | Remove untracked files (`git clean -fd`)          |
| `fs.clean_artifacts` | mutation     | Remove build artifact directories                 |
| `git.tag`            | mutation     | Tag HEAD as the last active state                 |
| `archive.compress`     | mutation     | Create a compressed archive of the project        |
| `backup.upload`      | mutation     | Upload the archive to the backup server           |
| `local.delete`       | mutation     | Delete the local project directory                |
| `hook.<name>`        | configurable | Run a user-defined shell command                  |
| `notify.<name>`      | mutation     | Display a message and wait for confirmation       |

Full reference: [docs/user/actions.md](docs/user/actions.md)

## Managing multiple projects

```bash
# Track projects
frostx projects add ~/projects/my-app ~/projects/other-app
frostx projects add --scan ~/projects   # auto-discover all frostx projects

# Operate on all tracked projects at once
frostx projects list
frostx projects check
frostx projects run --dry-run

# Untrack a project
frostx projects rm ~/projects/old-app
```

Note: Remember when renaming or moving a project directory, to retrack it under the new path.

Quick retrack all: `frostx projects add --scan ~`

## Machine-readable output

All commands accept `--json` for JSON/NDJSON output suitable for scripting:

```bash
frostx --json check ~/projects/my-app
frostx --json run ~/projects/my-app     # streams one NDJSON line per action
frostx --json projects list
```

## Installation

```bash
# From source (requires Rust toolchain)
cargo install frostx --bin frostx

# Via Nix flake
nix run github:0xCCF4/frostx
```

## Documentation

| Document                                                 | Contents                              |
|----------------------------------------------------------|---------------------------------------|
| [docs/user/cli-reference.md](docs/user/cli-reference.md) | All CLI commands and flags            |
| [docs/user/frostx-toml.md](docs/user/frostx-toml.md)     | Full `frostx.toml` schema             |
| [docs/user/actions.md](docs/user/actions.md)             | Action reference                      |
| [docs/user/state.md](docs/user/state.md)                 | State storage and UUID collisions     |
| [docs/user/includes.md](docs/user/includes.md)           | Config library and include resolution |
| [docs/ONBOARDING.md](docs/ONBOARDING.md)                 | Developer onboarding guide            |
| [docs/api/architecture.md](docs/api/architecture.md)     | Internal architecture reference       |

## On the use of AI

This project was a long-standing concept and half finished prototype before any AI tools were used. When I recently
tried Claude for the first time, I let it read the existing code and documentation, and in a supervised manner,
used it to bring to project into a usable state. Note that every change was carefully reviewed and edited afterward by
myself @0xCCF4.

I did not bother to delete the CLAUDE.md file, since I in fact used Claude to fill some holes in both the documentation
and code; and properly would not have picked up the project again after letting it rest so long.

## License

Due to the use of AI in the development process, I cannot confidently assert that this project is free
of "inspiration" from others' work, but I am not aware of any equivalent project, from which, AI might have copied
major parts. Since the idea, starting code base, and so on where given by myself, I am for now releasing this project
under the GPLv3 license into the open source community.

## Contributions

Contributions are welcome! Please open an issue or submit a pull request.

