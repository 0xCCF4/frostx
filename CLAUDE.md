# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**frostx** is a CLI tool for managing project lifecycle based on filesystem activity. A folder becomes a managed project
by placing a `frostx.toml` configuration file inside it. The tool scans projects for inactivity and executes configured
action pipelines.

## Development Commands

### Rust

```bash
cargo build                          # debug build
cargo build --release                # release build
cargo test                           # all tests
cargo test <test_name>               # single test by name
cargo test --test <integration_file> # single integration test file
cargo fmt --check                    # check formatting (CI)
cargo fmt                            # apply formatting
cargo clippy -- -W clippy::pedantic  # lints (must pass before completing any feature)
cargo clippy --all-targets -- -D warnings -W clippy::pedantic # lints (must pass before completing any feature)
```

### Nix

```bash
nix build                  # build via flake (must always succeed)
nix flake check            # run nix checks including unit tests
nix develop                # enter dev shell
```

### Quality gates (run after every change)

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings -W clippy::pedantic && cargo clippy -- -W clippy::pedantic && cargo test && nix build && nix flake check
```

## Architecture

### Core concepts

- **Project**: a directory containing `frostx.toml`. Each project is assigned a UUID on `init` (stored in
  `frostx.toml`). `frostx.toml` is never written to after `init` - it is safe to commit.
- **Config library**: a directory of plain TOML files (default: `~/.config/frostx/library/`) managed manually by the
  user. Projects reference entries by name, relative path, or absolute path via `include`.
  See [docs/user/includes.md](docs/user/includes.md).
- **Inactivity pipeline**: a sequence of `[[rule]]` blocks in `frostx.toml`, each specifying an `after` duration and a
  list of `actions`. Rules execute sequentially in declaration order; if any action in a rule fails, that rule fails and
  all subsequent rules are skipped for that run. On the next run, the pipeline retries from the first eligible rule
  whose threshold is still met, skipping already-completed mutation actions.

### `frostx.toml` schema

Full reference: [docs/user/frostx-toml.md](docs/user/frostx-toml.md) - full action
reference: [docs/user/actions.md](docs/user/actions.md)

```toml
id = "uuid-v4"                          # auto-assigned on init, never changed

include = [
    "archive-after-1y", # library entry: $FROSTX_LIBRARY/archive-after-1y.toml
    "./team-defaults.toml", # relative to project dir
    "/shared/base-rules.toml", # absolute path
]

[config.backup]
server = "rsync://backup.example.com/projects"

[[rule]]
after = "90d"
actions = [
    "git.check_clean", # check: fail if uncommitted changes exist
    "git.check_pushed", # check: auto-fetches, then fails if unpushed commits exist
    "backup.check", # check: fail if not on backup server
]

[[rule]]
after = "180d"
actions = [
    "git.clean", # mutation: remove untracked files (git clean -fd)
    "fs.clean_artifacts", # mutation: remove known build dirs before archiving
    "git.tag", # mutation: tag HEAD as last active state
    "archive.tar_gz", # mutation: create archive
    "backup.upload", # mutation: upload to backup server
    "backup.verify", # check: confirm upload integrity
    "local.delete", # mutation: remove local copy (always confirms interactively)
]
```

### Extensibility

Actions are the primary extension point of frostx. Adding a new static action requires only:

1. Implementing the `Action` trait in a new file under `src/actions/`.
2. Adding an entry to that module's `pub const REGISTRY: &[(&str, ActionFactory)]`.

If the action belongs to a new module category, add the module's `REGISTRY` to `ALL_REGISTRIES` in
`src/actions/mod.rs`. No changes to the pipeline engine, config parser, or CLI should be needed.

### Crate / module layout (intended)

```
src/
  lib.rs            - library root; re-exports all public modules
  main.rs           - CLI entry point (clap); thin adapter over ops/*
  ops/              - one module per subcommand; composes lib modules; public API for embedders
  config/           - frostx.toml parsing, include resolution, UUID management
  scanner/          - walk filesystem, determine last-modified timestamp
  pipeline/         - rule evaluation, action execution engine
  actions/          - one module per action category (git, archive, backup, fs, hook)
  backup/           - backend trait + implementations (rsync, ssh)
  output/           - data structs + human-readable and JSON/NDJSON renderers
  cli.rs            - clap type definitions shared with gen_man / gen_completions
tests/
  integration/      - end-to-end tests using temp directories
docs/
  user/             - user-facing documentation
  api/              - internal API / architecture docs
```

frostx is a library + binary in one crate. All business logic is in the library (`lib.rs` re-exports all modules
publicly). `main.rs` only parses args, calls `ops::*`, renders output, and exits. Third-party applications can call
`ops::*` directly with their own rendering.

### Key design decisions

- **Rule chain**: rules execute sequentially in declaration order. Within a rule, actions form a chain — if action N
  fails, actions N+1...end are skipped and the rule is marked failed. A failed rule stops evaluation: no subsequent
  rules run in that invocation. On the next run, already-completed mutation actions are skipped but the failed rule (and
  any after it) are retried from the top of their action list.
- **Include resolution**: `include` sources are resolved and merged before local config is evaluated. Rules from
  includes are prepended; `[config.*]` and `[group.*]` are merged with local values taking precedence. Nested includes
  are not supported.
- **State storage**: mutable state (last scan time, completed mutation actions) lives in
  `$XDG_DATA_HOME/frostx/<uuid>.toml` (default `~/.local/share/frostx/`), never in `frostx.toml`.
  See [docs/user/state.md](docs/user/state.md).
- **Check vs mutation actions**: check-type actions re-run every time. Mutation actions are recorded as completed and
  skipped on subsequent runs unless `--force` is passed.
- **UUID collision**: if two directories share a UUID (e.g. after a folder copy), frostx detects the path mismatch and
  aborts with exit code `4`. Resolution: `frostx init --force` in the copy.
- **Output**: all commands produce human-readable output by default. Pass `--json` globally for machine-readable output.
  `frostx run` uses NDJSON to stream action results in real time. Output format is the CLI's concern; `ops::*` functions
  return data structures and accept callbacks — they do not render anything directly.

## CLI Interface

Full command reference: [docs/user/cli-reference.md](docs/user/cli-reference.md)

| Command                                      | Description                                                       |
|----------------------------------------------|-------------------------------------------------------------------|
| `frostx init [PATH]`                         | Create `frostx.toml` with a generated UUID                        |
| `frostx check [PATH]`                        | Report inactivity and rule trigger status without running actions |
| `frostx run [PATH]`                          | Execute the inactivity pipeline and perform triggered actions     |
| `frostx scan [ROOT]`                         | Walk a directory tree and report status of all managed projects   |
| `frostx doctor [PATH]`                       | Validate `frostx.toml` without running anything                   |
| `frostx gc`                                  | Delete orphaned state files with no matching project              |
| `frostx projects list`                       | List all currently tracked projects                               |
| `frostx projects add [--scan DIR] [PATH...]` | Register projects in the state directory                          |
| `frostx projects rm PATH`                    | Unregister a project (delete its state file)                      |
| `frostx projects check`                      | Run `check` on every tracked project                              |
| `frostx projects run`                        | Run the inactivity pipeline for every tracked project             |

All commands accept `--dry-run` / `-n` and `--json`. `PATH` defaults to the current directory.

## Code Quality Rules

- **No panicking calls in runtime code.** `.unwrap()`, `.expect()`, `panic!()`, `unreachable!()`, `todo!()`, and
  `unimplemented!()` are forbidden in production paths (any code outside `#[cfg(test)]` blocks). Use `Result`-based
  error propagation with `?` instead. The only exception is `.expect()` when the absence of a value is a
  compile-time-provable invariant - in that case the expect message must state the invariant, not just a description.

## Documentation Requirements

- All public Rust items must have `///` doc comments.
- `docs/user/` covers CLI flags, `frostx.toml` reference, actions, state, and includes.
- `docs/api/` covers internal module contracts and the `Action` trait interface.

## Testing Requirements

Every feature needs:

1. Rust unit tests (`#[cfg(test)]` in the relevant module).
2. At least one integration test in `tests/integration/` that exercises the feature end-to-end with a temporary
   directory.

Tests must pass before a feature is considered complete.

## Language

Use **American English** throughout all source code, comments, and documentation. Common corrections: initialize (not
initialise), behavior (not behaviour), color (not colour), recognize (not recognise), normalize (not normalise).
