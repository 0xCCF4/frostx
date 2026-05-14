use super::{
    CheckOutput, DoctorOutput, GcOutput, InitOutput, ProjectAddOutput, ProjectRmOutput,
    ProjectsListOutput, RunActionOutput,
};
use owo_colors::OwoColorize;

/// Print the human-readable result of `frostx init`.
pub fn print_init(out: &InitOutput) {
    println!(
        "{:<12} {}",
        "initialized".green().bold(),
        out.path.display()
    );
    println!("{:<12} {}", "uuid".dimmed(), out.uuid);
}

/// Print the human-readable result of `frostx check`.
pub fn print_check(out: &CheckOutput) {
    println!(
        "{} {}  ({})",
        "project".bold(),
        out.project.cyan(),
        out.uuid.dimmed()
    );
    println!("{} {}", "path   ".bold(), out.path);
    println!(
        "{} {}",
        "inactive".bold(),
        format_seconds_as_str(out.inactive_seconds)
    );
    println!();

    for rule in &out.rules {
        let trigger_marker = if rule.triggered {
            "● TRIGGERED".red().bold().to_string()
        } else {
            "○ not yet".dimmed().to_string()
        };
        let remaining = if !rule.triggered && rule.remaining_seconds > 0 {
            format!(
                "  ({} remaining)",
                format_seconds_as_str(rule.remaining_seconds)
            )
        } else {
            String::new()
        };
        let name_part = rule
            .name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        println!(
            "rule #{}{}  after={}  {}{}",
            rule.index, name_part, rule.after, trigger_marker, remaining
        );
        for action in &rule.actions {
            let (marker, name_str) = match action.status.as_str() {
                "ok" | "completed" => ("  ✓".green().to_string(), action.name.clone()),
                "failed" => (
                    "  ✗".red().to_string(),
                    action.name.as_str().red().to_string(),
                ),
                "skipped" => (
                    "  –".dimmed().to_string(),
                    action.name.as_str().dimmed().to_string(),
                ),
                _ => ("  ?".to_string(), action.name.clone()),
            };
            println!("{} {:<28} — {}", marker, name_str, action.message.dimmed());
        }
        println!();
    }
}

/// Print one action result line from `frostx run`.
pub fn print_run_action(out: &RunActionOutput) {
    let prefix = match out.rule_name.as_deref() {
        Some(name) => format!("[rule {}: {name}]", out.rule),
        None => format!("[rule {}]", out.rule),
    };
    match out.status.as_str() {
        "ok" | "completed" => println!(
            "{} {} {} — {}",
            prefix.dimmed(),
            "✓".green().bold(),
            out.action.bold(),
            out.message.dimmed()
        ),
        "failed" => println!(
            "{} {} {} — {}",
            prefix.dimmed(),
            "✗".red().bold(),
            out.action.red().bold(),
            out.message.red()
        ),
        "skipped" => println!(
            "{} {} {} — {}",
            prefix.dimmed(),
            "–".dimmed(),
            out.action.dimmed(),
            out.message.dimmed()
        ),
        "dry_run" => println!(
            "{} {} {} — {}",
            prefix.dimmed(),
            "~".yellow(),
            out.action.yellow(),
            out.message.dimmed()
        ),
        _ => println!("{} {} — {}", prefix, out.action, out.message),
    }
}

/// Print the human-readable result of `frostx doctor`.
pub fn print_doctor(out: &DoctorOutput) {
    if out.valid && out.warnings.is_empty() {
        println!("{}", "✓ configuration is valid".green().bold());
        return;
    }
    for e in &out.errors {
        println!(
            "{} {} — {}",
            "error".red().bold(),
            e.field.bold(),
            e.message
        );
    }
    for w in &out.warnings {
        println!(
            "{} {} — {}",
            "warning".yellow().bold(),
            w.field.bold(),
            w.message
        );
    }
    if out.valid {
        println!("{}", "✓ valid with warnings".yellow());
    } else {
        println!("{}", "✗ configuration has errors".red().bold());
    }
}

/// Print the human-readable result of `frostx gc`.
pub fn print_gc(out: &GcOutput) {
    if out.orphaned.is_empty() {
        println!("{}", "no orphaned state files found".green());
        return;
    }
    for entry in &out.orphaned {
        let reason = match entry.reason.as_str() {
            "path_missing" => "path no longer exists".red().to_string(),
            "uuid_mismatch" => "UUID mismatch".yellow().to_string(),
            other => other.to_string(),
        };
        println!(
            "{} {}  ({}{})",
            "orphaned".yellow().bold(),
            entry.state_file.bold(),
            reason,
            if entry.path.is_empty() {
                String::new()
            } else {
                format!(": {}", entry.path)
            }
        );
    }
    println!();
    if out.removed == 0 {
        println!(
            "{} orphaned state {} found. Run without --dry-run to delete.",
            out.orphaned.len(),
            if out.orphaned.len() == 1 {
                "file"
            } else {
                "files"
            }
        );
    } else {
        println!(
            "{} orphaned state {} removed.",
            out.removed,
            if out.removed == 1 { "file" } else { "files" }
        );
    }
}

/// Print the human-readable result of `frostx projects list`.
pub fn print_projects_list(out: &ProjectsListOutput) {
    if out.projects.is_empty() {
        println!("{}", "no tracked projects".dimmed());
        return;
    }
    for p in &out.projects {
        let scan = p
            .last_scan
            .as_deref()
            .unwrap_or("never")
            .dimmed()
            .to_string();
        println!(
            "{} {}  (last scan: {})",
            p.uuid.dimmed(),
            p.path.bold(),
            scan
        );
    }
}

/// Print the human-readable result of `frostx projects add`.
pub fn print_projects_add(out: &ProjectAddOutput) {
    for p in &out.added {
        println!(
            "{} {} {}",
            "tracked".green().bold(),
            p.path.bold(),
            p.uuid.dimmed()
        );
    }
    for s in &out.skipped {
        println!(
            "{} {} — {}",
            "skipped".yellow().bold(),
            s.path.bold(),
            s.reason
        );
    }
}

/// Print the human-readable result of `frostx projects rm`.
pub fn print_projects_rm(out: &ProjectRmOutput) {
    println!(
        "{} {} {}",
        "untracked".yellow().bold(),
        out.path.bold(),
        out.uuid.dimmed()
    );
}

/// Print an error message to stderr.
pub fn print_error(message: &str) {
    eprintln!("{} {}", "error:".red().bold(), message);
}

/// Formats seconds as string like "5 minutes", "2 hours", or "3 days", choosing the largest appropriate unit.
#[must_use]
pub fn format_seconds_as_str(seconds: i64) -> String {
    if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} hours", seconds / 3600)
    } else {
        format!("{} days", seconds / 86_400)
    }
}
