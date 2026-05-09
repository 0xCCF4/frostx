# Architecture Reference

Internal module contracts, data flow, and the `Action` trait interface.

## Library / binary split

frostx is structured as a single crate that exposes both a library (`lib.rs`) and the `frostx` CLI binary (`main.rs`).
All business logic lives in the library; the binary is a thin adapter that parses CLI arguments and delegates to
`ops::*`.

Custom applications can depend on the `frostx` crate and call `ops::*` functions directly, providing their own rendering
callbacks.

## Module dependency graph

```
main.rs  (CLI adapter only - no business logic)
  │
  └─ ops/*            (high-level operations; one module per subcommand)
       ├─ config/*    (parse frostx.toml, resolve includes, read/write state)
       ├─ scanner     (filesystem walk → last-modified timestamp)
       ├─ pipeline    (rule evaluation + action execution)
       │    └─ actions/* (Action trait implementations)
       │         └─ backup/* (rsync/SSH backends)
       └─ output/*   (data structs + human-readable / JSON/NDJSON renderers)
```

`error.rs` is a leaf dependency referenced by all modules. No module imports from `ops/*` - that layer only depends
downward.

## Module contracts

### `config`

**Inputs:** path to `frostx.toml`, optional config override path, library directory.

**Outputs:**

- `ProjectConfig` - fully resolved, include-merged configuration. All `group.*` references are still unexpanded at this
  point; callers invoke `config.expand_groups()` before iterating rules.
- `ProjectState` - per-UUID mutable state (last scan, completed mutations).

**Key invariant:** `frostx.toml` is never written after `init`. State writes go only to the separate state file.
`ProjectConfig` is therefore safe to hold as a shared reference.

**Include resolution** (`config/mod.rs`): include sources are resolved and merged before the local config is evaluated.
Rules from includes are prepended; `[config.*]` and `[group.*]` are merged with local values taking precedence. Nested
includes are currently not supported.

---

### `scanner`

**Input:** a directory path.

**Output:** `DateTime<Utc>` - the most-recent modification time of any file in the directory tree, ignoring hidden
directories and the `frostx.toml` file itself.

**Used by:** `ops/check.rs` and `ops/run.rs`.

---

### `pipeline`

Two public functions:

#### `evaluate(config, state, last_modified) -> Result<Vec<RuleOutcome>>`

Pure read - does not execute actions or mutate state. Computes, for each rule:

- `triggered`: whether `last_modified` is older than `rule.after`
- `remaining_seconds`: how many seconds until the threshold elapses
- `action_outcomes`: for triggered rules, whether each action is pending or already completed

Used by `frostx check`.

#### `run(config, state, path, last_modified, opts, on_action) -> Result<Vec<RuleOutcome>>`

Executes the pipeline:

1. Expands group references (`config.expand_groups()`).
2. For each triggered rule, iterates actions in order.
3. Skips completed mutations (unless `opts.force`).
4. Calls `actions::create(name, config)` to instantiate the action.
5. Calls `action.run(ctx)`.
6. On success, records completed mutations via `state.mark_completed()`.
7. On failure, sets `chain_failed = true`; subsequent actions in the same rule get `ActionStatus::Skipped`.
8. Calls `on_action(rule_index, &outcome)` after every action - callers use this to stream output.

Rules are independent: a failed chain in rule 1 does not affect rule 2.

---

### `actions`

#### The `Action` trait

```rust
pub trait Action: Send + Sync {
    fn name(&self) -> &'static str;
    fn kind(&self) -> ActionKind;
    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
}
```

`ActionContext` carries:

| Field          | Type             | Description                                              |
|----------------|------------------|----------------------------------------------------------|
| `project_path` | `&Path`          | Absolute path to the project root                        |
| `config`       | `&ProjectConfig` | Parsed `frostx.toml` (read-only)                         |
| `dry_run`      | `bool`           | When true, report what would happen without side effects |
| `yes`          | `bool`           | When true, skip interactive prompts                      |

`ActionOutcome` has a `status: ActionStatus` and a human-readable `message: String`. Construct with the convenience
methods: `ActionOutcome::ok`, `::failed`, `::skipped`, `::dry_run`.

#### Registration

`actions::create(name, config)` is the single dispatch point. Adding a new action requires only:

1. Implementing `Action` in a source file under `src/actions/`.
2. Adding a `match` arm in `create()`.

No other code needs to change.

---

### `backup`

A `Backend` trait with `upload`, `verify`, and `check` methods. Implementations:

- `rsync` - shells out to `rsync`; URL scheme `rsync://`
- (future) `ssh` - URL scheme `ssh://`

The backup actions in `src/actions/backup.rs` delegate to the appropriate backend based on the `server` URL in
`[config.backup]`.

---

### `ops`

Each module in `ops/` composes `config`, `scanner`, `pipeline`, and `output` into a single callable operation. All `ops`
functions accept `FrostxOpts` and return `Result<T, FrostxError>` (never an exit code - that is the binary's concern).

Key functions:

| Function                 | Returns                               | Notes                                                                           |
|--------------------------|---------------------------------------|---------------------------------------------------------------------------------|
| `ops::init::execute`     | `Result<InitOutput>`                  | Creates `frostx.toml`                                                           |
| `ops::check::gather`     | `Result<CheckOutput>`                 | Evaluates rules, saves state                                                    |
| `ops::run::execute`      | `Result<bool>`                        | Runs pipeline; `bool` = had failures. Accepts an `ActionCallback` for streaming |
| `ops::scan::execute`     | `Result<Vec<CheckOutput>>`            | Walks a tree, silently skips load failures                                      |
| `ops::doctor::execute`   | `Result<DoctorOutput>`                | Validates config                                                                |
| `ops::gc::execute`       | `Result<GcOutput>`                    | Removes orphaned state files                                                    |
| `ops::projects::run_all` | `(bool, Vec<(PathBuf, FrostxError)>)` | Streams via `&dyn Fn(&Path, usize, &ActionOutcome)`                             |

`FrostxOpts` holds behavioral options (`dry_run`, `yes`, `state_dir`, etc.) but deliberately has no `format`/output
field. Output rendering is the caller's responsibility.

---

### `output`

Two renderers with identical function signatures:

| Module          | Target        | Format            |
|-----------------|---------------|-------------------|
| `output::human` | stdout/stderr | ANSI-colored text |
| `output::json`  | stdout/stderr | JSON / NDJSON     |

Output structs are defined in `output/mod.rs` and derive `serde::Serialize`. The `frostx run` command streams one NDJSON
object per action via `print_run_action`; all other commands emit a single JSON object at the end.

The active renderer is selected in `main()` from the `--json` flag and passed as a captured `OutputFormat` value into
per-command closures. It is not part of `FrostxOpts`.

---

## Data flow: `frostx run`

```
main() parses CLI
  │
  ├─ FrostxOpts built (dry_run, state_dir, ...)
  ├─ format: OutputFormat determined from --json flag
  │
  └─ ops::run::execute(RunArgs, &FrostxOpts, &on_action_callback)
        │
        ├─ config::load(path)            → ProjectConfig
        ├─ config::state::load(uuid)     → ProjectState
        ├─ scanner::scan(path)           → ScanResult (last_modified)
        │
        ├─ pipeline::run(config, &mut state, path, ts, opts, on_action)
        │     │
        │     └─ for each triggered rule:
        │           for each action:
        │             actions::create(name, config) → Box<dyn Action>
        │             action.run(ctx) → ActionOutcome
        │             state.mark_completed(...)       [mutations only]
        │             on_action(rule_idx, &outcome) → caller renders line
        │
        └─ state.save(state_dir, uuid)
```

`on_action` is provided by `main()` and captures `format`; it constructs a `RunActionOutput` struct and calls the
appropriate renderer.

## Error handling

All errors propagate as `FrostxError` via `?`. `FrostxError::exit_code()` maps each variant to a documented exit code:

| Code | Meaning                                                       |
|------|---------------------------------------------------------------|
| 0    | Success                                                       |
| 1    | General error (config, action failure, I/O)                   |
| 2    | Warnings only (used by `doctor`)                              |
| 3    | Project not initialized - no `frostx.toml` found              |
| 4    | UUID collision - project is a copy of another tracked project |

`ops::*` functions return `FrostxError` variants; `main()` maps them to exit codes and calls the appropriate
`output::print_error` renderer. No panics in production paths.

## State file lifecycle

```
frostx init        → creates <uuid>.toml with empty state
frostx check/run   → loads state, updates project_path + last_scan, saves
frostx projects rm → deletes <uuid>.toml
frostx gc          → deletes <uuid>.toml files with no matching project_path
```

State files are named by UUID so they survive project directory renames.
