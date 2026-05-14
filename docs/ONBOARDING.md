# frostx Developer Onboarding

Welcome to frostx. This guide gets you from zero to productive in under 30 minutes.

## What frostx does

frostx monitors project directories for inactivity. A project is any directory containing a `frostx.toml` file. Once a
configured inactivity threshold elapses (e.g. "90 days since any file was modified"), frostx executes a pipeline of
**actions** - checking git state, creating archives, uploading backups, deleting local copies. Think of it as a
cron-triggered lifecycle manager rooted in filesystem time.

## Repository layout

```
src/
  lib.rs            - library root; re-exports all public modules
  main.rs           - CLI entry point (clap); thin adapter over ops/*
  ops/              - one module per subcommand; compose lib modules into operations
  config/           - parse frostx.toml, resolve includes, read/write state
  scanner/          - walk filesystem to find last-modified timestamp
  pipeline/         - evaluate rule thresholds and execute action chains
  actions/          - one module per action category; all implement Action trait
  backup/           - rsync / SSH backend implementations
  output/           - data structs + human-readable (colored) and JSON/NDJSON renderers
  error.rs          - FrostxError enum and exit-code constants
  cli.rs            - clap type definitions (shared with gen_man / gen_completions)

tests/
  integration/      - end-to-end tests using temp directories (tempfile crate)

docs/
  user/             - user-facing: CLI reference, frostx.toml, actions, state, includes
  api/              - internal: module contracts, Action trait, architecture
```

frostx is structured as a library + binary in a single crate. All business logic lives in the library (`lib.rs`
re-exports everything); `main.rs` only parses arguments and calls `ops::*`. Custom applications can depend on the
`frostx` crate and call `ops::*` directly with their own output callbacks.

## Building and testing

```bash
cargo build                          # debug build
cargo build --release                # release build
cargo test                           # all unit + integration tests
cargo test <name>                    # single test by name substring
cargo test --test <file>             # single integration test file
cargo fmt                            # apply formatting
cargo fmt --check                    # formatting check (CI gate)
cargo clippy -- -W clippy::pedantic  # lints (must pass before completing any feature)
nix build                            # build via flake (must always succeed)
nix flake check                      # run nix checks
nix develop                          # drop into a dev shell with all tools
```

**Quality gate** - run after every change:

```bash
cargo fmt --check && cargo clippy -- -W clippy::pedantic && cargo test && nix build && nix flake check
```

## Code quality rules

- **No panicking calls in production code.** `.unwrap()`, `.expect()`, `panic!()`, `unreachable!()`, `todo!()`,
  `unimplemented!()` are forbidden outside `#[cfg(test)]` blocks. Use `Result` + `?` everywhere. Only exception: when a
  panic is provably impossible - document why in a comment.
- All public Rust items must have `///` doc comments.
- All modules should have a `//!` doc comment at the top.

## Core concepts

### Config vs state

`frostx.toml` is read-only after `frostx init`. It lives in the project directory and is safe to commit.

Mutable state (last scan time, completed mutations) lives in `$XDG_DATA_HOME/frostx/<uuid>.toml`. The filename is the
project's UUID, so state survives the project folder being renamed.

### Check vs mutation actions

Actions have a `kind()`:

| Kind       | Behavior                                                                 |
|------------|--------------------------------------------------------------------------|
| `Check`    | Re-runs on every invocation; never recorded as completed                 |
| `Mutation` | Recorded as completed after success; skipped on re-runs unless `--force` |

### Rule evaluation

Rules execute sequentially in declaration order. Within a rule, actions are a chain: if action N fails, actions N+1...
are skipped and the rule fails. A failed rule stops the pipeline — no subsequent rules run in that invocation. On the
next run the pipeline retries from the first eligible rule, skipping already-completed mutation actions within it.

## The Action trait - adding new actions

This is the primary extension point. There are two steps:

### Step 1 - implement the trait

Create a new struct in the appropriate submodule of `src/actions/` (or a new submodule if the category doesn't exist
yet):

```rust
use crate::actions::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::error::FrostxError;

pub struct MyNewCheck;

impl Action for MyNewCheck {
    fn name(&self) -> &'static str {
        "mycat.my_check"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Check  // or ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        // ctx.project_path  - path to the project root
        // ctx.config        - parsed frostx.toml
        // ctx.dry_run       - true when --dry-run was passed
        // ctx.yes           - true when --yes was passed
        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run("would have checked something"));
        }
        // ... do work ...
        Ok(ActionOutcome::ok("everything is fine"))
    }
}
```

Use `ActionOutcome::ok`, `::failed`, `::skipped`, or `::dry_run` to construct outcomes - they set the correct
`ActionStatus` variant.

### Step 2 - register the name

Add an entry to the `REGISTRY` constant in the same module file:

```rust
use super::ActionFactory;

pub const REGISTRY: &[(&str, ActionFactory)] = &[
    // existing entries ...
    ("mycat.my_check", |_| Ok(Box::new(MyNewCheck))),
];
```

If the action needs config at construction time, pass `config` to the constructor:

```rust
("mycat.my_action", |config| Ok(Box::new(MyAction::new(config)))),
```

If the constructor is fallible (returns `Result`), propagate the error with `?`:

```rust
("mycat.my_action", |config| Ok(Box::new(MyAction::new(config)?))),
```

If you create a new module category, add its `REGISTRY` to the `ALL_REGISTRIES` list in `src/actions/mod.rs`. That is
the only change needed outside the new module.

For **dynamic** action categories (user-named like `hook.<name>`) add a `strip_prefix` branch in `create()` in
`src/actions/mod.rs` instead of a `REGISTRY` entry.

That's the entire registration. No changes needed elsewhere in the pipeline or CLI.

### Config access

If the action needs parameters from `frostx.toml`, add a field to the relevant `[config.*]` section in
`src/config/project.rs` and read it from `ctx.config`. See `BackupConfig`, `ArchiveConfig`, `FsConfig`, and
`NotifyConfig` as examples. For user-defined shell commands, use the existing `hook` mechanism (`[config.hook.<name>]`).

## How state is managed

`src/config/state.rs` owns `ProjectState`. Key operations:

- `ProjectState::load(dir, uuid)` - read from disk (or return default if missing)
- `ProjectState::save(dir, uuid)` - write to disk
- `state.is_completed(rule_index, action_name)` - query per-rule completion
- `state.mark_completed(rule_index, action_name)` - record a completed mutation

The pipeline engine (`src/pipeline/mod.rs`) calls these; individual actions never touch state directly.

## Operations and output

Each subcommand has a module in `src/ops/`. Operation functions accept `FrostxOpts` (holds `dry_run`, `state_dir`, etc.)
and return `Result<T, FrostxError>` — never exit codes. Rendering is the caller's concern.

Output structs live in `src/output/mod.rs`. Add a new struct there when a new command needs structured output, then add
`print_*` functions to both `output/human.rs` and `output/json.rs`.

`main.rs` constructs output format from the `--json` flag, calls the appropriate `ops::*` function, renders the result,
and exits with the mapped code. The `OutputFormat` value is captured into closures (e.g. the `on_action` callback for
`run`) — it is not part of `FrostxOpts`.

## Writing tests

Every new feature needs:

1. **Unit tests** (`#[cfg(test)]`) in the relevant source module.
2. **Integration test** in `tests/integration/` - use `tempfile::tempdir()` for scratch space. Look at
   `tests/integration/init.rs` for the setup pattern.

Integration tests invoke `ops::*` functions directly (not the binary) so they get proper `Result` propagation without
subprocess overhead.

## Further reading

| Document                                            | Contents                                      |
|-----------------------------------------------------|-----------------------------------------------|
| [docs/user/frostx-toml.md](user/frostx-toml.md)     | Full `frostx.toml` schema reference           |
| [docs/user/actions.md](user/actions.md)             | All built-in action names and behavior        |
| [docs/user/cli-reference.md](user/cli-reference.md) | All CLI commands and flags                    |
| [docs/user/state.md](user/state.md)                 | State file format and UUID collision handling |
| [docs/user/includes.md](user/includes.md)           | Include resolution and config library         |
| [docs/api/architecture.md](api/architecture.md)     | Internal module contracts and data flow       |
