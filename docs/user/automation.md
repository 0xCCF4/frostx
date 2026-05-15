# Automating frostx with shell startup integration

frostx can run your inactivity pipelines automatically once per day by hooking
into your shell's startup file (`.bashrc`, `.zshrc`, etc.).

## How it works

The `--daily` flag on `frostx projects run` and `frostx projects check` adds
two automatic guards:

1. **Interactive-terminal check** - the command exits immediately when stdout is
   not a TTY (e.g. in a cron job, CI pipeline, or non-interactive shell). This
   prevents surprise side effects in automated environments.

2. **24-hour cooldown** - after a successful run, frostx records a timestamp
   in `$XDG_DATA_HOME/frostx/daily.toml` (default `~/.local/share/frostx/`).
   On subsequent invocations within the same 24-hour window the command exits
   silently without running anything (returning cached results).  `projects run --daily` and
   `projects check --daily` maintain **independent** timestamps, so using one
   does not suppress the other.

The net effect: the first terminal you open each day triggers the pipeline; all
later terminals that day are instant no-ops.

## Quickstart

### 1. Register your projects

Before automation is useful, tell frostx which projects to manage. Either add
individual projects or scan a directory tree:

```sh
# Add one project at a time
frostx projects add ~/code/my-project

# Or discover every frostx project under a root directory
frostx projects add --scan ~/code
```

Verify the list at any time:

```sh
frostx projects list
```

### 2. Add the hook to your shell startup file

```bash
# frostx – run inactivity pipelines once per day
frostx projects run --daily
```

## Variants

### Check only (no actions)

If you want a daily status report without executing any actions, use
`frostx projects check --daily` instead:

```bash
# frostx – show daily inactivity status without running actions
frostx projects check --daily
```

### Quiet mode

Pass `--quiet` to suppress all output when nothing is triggered:

```bash
frostx projects run --daily --quiet
```

### Verbose mode

Pass `--verbose` (or `-v`) to see extra detail:

```bash
frostx projects run --daily --verbose
```

## State file

The daily timestamp is stored at:

```
$XDG_DATA_HOME/frostx/daily.toml
# on most Linux systems:
~/.local/share/frostx/daily.toml
```
