//! Interactive prompts for `frostx init`.
//!
//! When stdin is a TTY and `--yes` is not set, `run_init_questionnaire` collects
//! a project name, description, include selections, and template variable values
//! from the user via the terminal. Callers that pass `interactive = false` receive
//! `None` and should fall back to CLI-provided defaults.

use crate::config::include::extract_template_vars;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};
use std::collections::HashMap;
use std::io::IsTerminal as _;
use std::path::Path;

/// Results collected by the `frostx init` questionnaire.
pub struct InitPromptResult {
    /// Optional human-readable project name.
    pub name: Option<String>,
    /// Optional project description.
    pub description: Option<String>,
    /// Library entries chosen by the user.
    pub includes: Vec<String>,
    /// Template variable values for `{{key}}` substitution.
    pub template: HashMap<String, String>,
}

/// Run the interactive `frostx init` questionnaire.
///
/// Returns `None` when the terminal is not interactive (`!stdin.is_terminal()`)
/// or when `interactive` is `false` (i.e. `--yes` was passed).  In those cases
/// the caller should use CLI-provided defaults instead.
///
/// `cli_includes` contains any `--include` entries already given on the command
/// line; they are pre-selected in the multi-select prompt.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if a prompt cannot be rendered.
pub fn run_init_questionnaire(
    cli_includes: &[String],
    library_dir: &Path,
    interactive: bool,
) -> std::io::Result<Option<InitPromptResult>> {
    if !interactive || !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let theme = ColorfulTheme::default();

    // --- project name ---
    let name_raw: String = Input::with_theme(&theme)
        .with_prompt("Project name (leave empty to skip)")
        .allow_empty(true)
        .interact_text()?;
    let name = if name_raw.trim().is_empty() {
        None
    } else {
        Some(name_raw.trim().to_owned())
    };

    // --- description ---
    let desc_raw: String = Input::with_theme(&theme)
        .with_prompt("Project description (leave empty to skip)")
        .allow_empty(true)
        .interact_text()?;
    let description = if desc_raw.trim().is_empty() {
        None
    } else {
        Some(desc_raw.trim().to_owned())
    };

    // --- library includes ---
    let library_entries = discover_library_entries(library_dir);
    let includes = if library_entries.is_empty() {
        cli_includes.to_vec()
    } else {
        let defaults: Vec<bool> = library_entries
            .iter()
            .map(|e| cli_includes.contains(e))
            .collect();
        let selections = MultiSelect::with_theme(&theme)
            .with_prompt("Select library templates to include (space to toggle, enter to confirm)")
            .items(&library_entries)
            .defaults(&defaults)
            .interact()?;
        selections
            .into_iter()
            .map(|i| library_entries[i].clone())
            .collect()
    };

    // --- template variables ---
    let template = collect_template_vars(&theme, &includes, library_dir)?;

    Ok(Some(InitPromptResult {
        name,
        description,
        includes,
        template,
    }))
}

/// Scan `library_dir` for `.toml` files and return their stem names (without extension).
fn discover_library_entries(library_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(library_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    names
}

/// Read each selected include file and prompt for any unresolved `{{variable}}`
/// placeholders, returning a map of variable → value.
fn collect_template_vars(
    theme: &ColorfulTheme,
    includes: &[String],
    library_dir: &Path,
) -> std::io::Result<HashMap<String, String>> {
    let mut template: HashMap<String, String> = HashMap::new();

    for include in includes {
        let path = resolve_include_path(include, library_dir);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for var in extract_template_vars(&content) {
            if template.contains_key(&var) {
                continue;
            }
            let value: String = Input::with_theme(theme)
                .with_prompt(format!(
                    "Value for template variable `{var}` (used in {include})"
                ))
                .allow_empty(true)
                .interact_text()?;
            template.insert(var, value.trim().to_owned());
        }
    }

    Ok(template)
}

fn resolve_include_path(source: &str, library_dir: &Path) -> std::path::PathBuf {
    if source.starts_with('/') || source.starts_with("./") || source.starts_with("../") {
        std::path::PathBuf::from(source)
    } else {
        library_dir.join(format!("{source}.toml"))
    }
}
