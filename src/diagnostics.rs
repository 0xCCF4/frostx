//! Rich error formatting for configuration and action diagnostics.
//!
//! Provides source-snippet TOML error formatting (the `toml` crate already
//! renders a snippet; we prepend the filename) and nearest-match suggestions
//! for unknown action names via Levenshtein edit distance.

use crate::actions;
use std::fmt::Write as _;
use std::path::Path;

/// Levenshtein edit distance between two strings (O(m·n) time, O(n) space).
fn levenshtein(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let (m, n) = (ac.len(), bc.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if ac[i - 1] == bc[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Return up to 3 static action names closest to `name` by edit distance.
///
/// The threshold scales with name length so short typos surface near misses
/// while completely unrelated names are suppressed.
#[must_use]
pub fn suggest_actions(name: &str) -> Vec<&'static str> {
    let threshold = (name.len() / 3 + 2).min(6);
    let mut scored: Vec<(&'static str, usize)> = actions::all_static_actions()
        .iter()
        .map(|&a| (a, levenshtein(name, a)))
        .filter(|(_, d)| *d <= threshold)
        .collect();
    scored.sort_by_key(|&(_, d)| d);
    scored.truncate(3);
    scored.into_iter().map(|(a, _)| a).collect()
}

/// Format a TOML parse/deserialization error with the source file path prepended.
///
/// The `toml` crate (v1+) already renders a multi-line snippet with a caret
/// pointing at the offending token.  This function adds the filename so the
/// user knows which file is being described.
#[must_use]
pub fn format_toml_error(error: &toml::de::Error, path: &Path) -> String {
    format!("config error in {}:\n{error}", path.display())
}

/// Build the full human-readable message for an unknown action name.
///
/// Includes nearest-match suggestions and a compact columnar list of every
/// available static action, plus a note about dynamic categories.
#[must_use]
pub fn unknown_action_message(name: &str) -> String {
    let suggestions = suggest_actions(name);
    let mut msg = format!("unknown action '{name}'");

    if !suggestions.is_empty() {
        msg.push_str("\n\n  did you mean?");
        for s in &suggestions {
            let _ = write!(msg, "\n    {s}");
        }
    }

    // Compact three-column list of all static actions.
    let all = actions::all_static_actions();
    let col_w = all.iter().map(|s| s.len()).max().unwrap_or(0) + 2;
    msg.push_str("\n\n  available actions:");
    for (i, action) in all.iter().enumerate() {
        if i % 3 == 0 {
            msg.push_str("\n    ");
        }
        if i % 3 < 2 {
            let _ = write!(msg, "{action:<col_w$}");
        } else {
            msg.push_str(action);
        }
    }
    msg.push_str("\n  dynamic: hook.<name>  notify.<name>  group.<name>");
    msg
}

/// Short inline hint for an unknown action, suitable for single-line output.
#[must_use]
pub fn unknown_action_hint(name: &str) -> String {
    let suggestions = suggest_actions(name);
    if suggestions.is_empty() {
        format!("unknown action '{name}'")
    } else {
        format!(
            "unknown action '{name}' - did you mean: {}?",
            suggestions.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn levenshtein_insertion() {
        assert_eq!(levenshtein("abc", "abcd"), 1);
    }

    #[test]
    fn levenshtein_substitution() {
        assert_eq!(levenshtein("git.check_clan", "git.check_clean"), 1);
    }

    #[test]
    fn suggest_close_typo() {
        let s = suggest_actions("git.check_clan");
        assert!(
            s.contains(&"git.check_clean"),
            "expected check_clean in {s:?}"
        );
    }

    #[test]
    fn suggest_no_match_for_garbage() {
        let s = suggest_actions("zzzzzzzzzzzzz");
        assert!(
            s.is_empty(),
            "expected no suggestions for garbage, got {s:?}"
        );
    }

    #[test]
    fn unknown_action_message_contains_name() {
        let msg = unknown_action_message("git.chek_clean");
        assert!(msg.contains("git.chek_clean"));
        assert!(msg.contains("did you mean"));
        assert!(msg.contains("available actions"));
    }
}
