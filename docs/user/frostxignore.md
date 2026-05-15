# `.frostxignore`

Place a `.frostxignore` file at the root of a project directory to exclude paths
from frostx's inactivity scanner. Paths that match are not counted when
determining the project's last-modified timestamp, so they will not reset the
inactivity clock.

## Format

`.frostxignore` follows full [gitignore syntax](https://git-scm.com/docs/gitignore):

- Blank lines and lines starting with `#` are ignored.
- A leading `/` anchors the pattern to the project root.
- A trailing `/` matches directories only.
- `*` matches anything except `/`; `**` matches across directory boundaries.
- A leading `!` negates a pattern (re-includes a previously excluded path).

```
# Compiled output
dist/
target/
build/

# Log files
*.log

# Python bytecode
__pycache__/
*.pyc

# Only at the project root (not in subdirectories)
/tmp-scratch/
```

## Scope

`.frostxignore` is loaded from the **project root only**.  Unlike `.gitignore`,
nested ignore files inside subdirectories are not supported.

## Hardcoded exclusions

The following are always excluded regardless of `.frostxignore`:

| Path | Reason |
|------|--------|
| `.git/` | Git metadata — updated by `git fetch`, `git gc`, etc. |
| `.jj/` | Jujutsu metadata — updated by `jj git fetch`, etc. |
| `frostx.toml` | Project configuration file |
| `.frostxignore` | This file itself |
