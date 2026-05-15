# Includes & Reuse

frostx allows `frostx.toml` files to include configuration fragments from external files. This enables shared pipelines
across projects without copy-pasting.

## Include Syntax

The `include` field accepts a list of sources resolved in declaration order:

```toml
include = [
    "archive-after-1y", # library entry: $FROSTX_LIBRARY/archive-after-1y.toml
    "./shared/git-checks.toml", # relative to the project directory
    "/home/user/base-rules.toml" # absolute path
]
```

| Format                    | Resolved as                                        |
|---------------------------|----------------------------------------------------|
| No path separators        | Library entry: `$FROSTX_LIBRARY/<name>.toml`       |
| Starts with `./` or `../` | Relative to the directory containing `frostx.toml` |
| Starts with `/`           | Absolute path                                      |

Includes are processed before the local file is evaluated. If a source cannot be found, frostx exits with an error.

## What an Included File Can Contain

An included file is a partial `frostx.toml`. It may define any combination of:

- `[[rule]]` blocks
- `[config.*]` sections
- `[group.*]` sections (see below)

It must **not** contain `id` or `include` (nested includes are not supported).

## Groups

A group is a named, reusable list of actions that can be referenced inside any rule's `actions` array.

**Defining a group** (in any included file or in `frostx.toml` itself):

```toml
[group.git_checks]
actions = ["git.check_clean", "git.check_pushed"]

[group.full_archive]
actions = [
    "git.clean",
    "fs.clean_artifacts",
    "git.tag",
    "archive.compress",
    "backup.upload",
    "backup.verify",
    "local.delete",
]
```

**Using a group** in a rule (prefix with `group.`):

```toml
[[rule]]
after = "90d"
actions = ["group.git_checks", "backup.check"]

[[rule]]
after = "180d"
actions = ["group.git_checks", "group.full_archive"]
```

Groups are expanded inline at evaluation time - the actions they contain behave identically to actions listed directly.
A group may reference other groups.

## Merge Semantics

| Element           | Behavior                                                                  |
|-------------------|---------------------------------------------------------------------------|
| `[[rule]]` blocks | Appended in include order, before local rules                             |
| `[config.*]`      | Merged; local values override included values for the same key            |
| `[group.*]`       | Merged; local group definitions override included ones with the same name |

Example: if a library file defines `[config.backup]` with a default server, a project can override just that key
locally:

```toml
# library file
[config.backup]
server = "rsync://default-backup.example.com/projects"

# frostx.toml - overrides only the server
include = ["company-defaults"]

[config.backup]
server = "rsync://my-backup.example.com/projects"
```

## Template Variables

Include files may contain `{{variable_name}}` placeholders that are replaced with values from the `[template]` table in
`frostx.toml` before the file is parsed as TOML.

### Defining placeholders in a library entry

`$FROSTX_LIBRARY/team-backup.toml`:

```toml
[config.backup]
server = "{{backup_server}}"

[[rule]]
after = "90d"
actions = ["backup.check"]

[[rule]]
after = "365d"
actions = ["backup.upload", "backup.verify", "local.delete"]
```

### Providing values in `frostx.toml`

```toml
id = "uuid-v4"

include = ["team-backup"]

[template]
backup_server = "rsync://myteam-backup.example.com/projects"
```

When frostx loads this project, it replaces `{{backup_server}}` with `"rsync://myteam-backup.example.com/projects"` and
parses the resulting TOML as the include fragment.

### Rules

- Variable names must consist of ASCII alphanumeric characters and underscores only.
- If an included file contains a `{{variable}}` placeholder with no matching entry in `[template]`, frostx exits with an
  error.
- The `[template]` table is only valid in `frostx.toml` - included files cannot define or override template values.
- `frostx init` automatically prompts for template variable values when the selected library entries contain placeholders
  (skipped when `--yes` or `--json` is passed).

---

## Example: Library Entry

`$FROSTX_LIBRARY/archive-after-1y.toml`:

```toml
[group.standard_checks]
actions = ["git.check_clean", "git.check_pushed", "backup.check"]

[group.full_archive]
actions = [
    "git.clean",
    "fs.clean_artifacts",
    "git.tag",
    "archive.compress",
    "backup.upload",
    "backup.verify",
    "local.delete",
]

[[rule]]
after = "90d"
actions = ["group.standard_checks"]

[[rule]]
after = "365d"
actions = ["group.standard_checks", "group.full_archive"]
```

A project using this library entry only needs:

```toml
id = "uuid-v4"

include = ["archive-after-1y"]

[config.backup]
server = "rsync://backup.example.com/projects"
```
