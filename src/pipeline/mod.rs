use crate::config::duration::Duration;
use crate::config::project::ProjectConfig;
use crate::config::state::ProjectState;
use crate::error::FrostxError;
use chrono::DateTime;
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Status of a single action execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ActionStatus {
    /// Check passed or action succeeded.
    Ok,
    /// Action or check failed - chain stops.
    Failed,
    /// Skipped because a preceding action failed.
    Skipped,
    /// Mutation was already completed in a previous run.
    Completed,
    /// Dry-run mode - action would have run.
    DryRun,
}

/// Outcome of a single action within a rule.
#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub name: String,
    pub status: ActionStatus,
    pub message: String,
}

/// Outcome of evaluating one rule.
#[derive(Debug, Clone)]
pub struct RuleOutcome {
    #[allow(dead_code)]
    pub index: usize,
    pub name: Option<String>,
    pub after: Duration,
    pub after_seconds: i64,
    pub triggered: bool,
    pub remaining_seconds: i64,
    pub action_outcomes: Vec<ActionOutcome>,
}

/// Options controlling pipeline execution.
pub struct RunOptions {
    pub dry_run: bool,
    pub force: bool,
    pub yes: bool,
    pub rule_filter: Option<usize>,
    pub action_filter: Option<String>,
}

/// Callback invoked after each action completes (used to stream output).
pub type ActionCallback<'a> = Box<dyn Fn(usize, Option<&str>, &ActionOutcome) + 'a>;

/// Evaluate which rules are triggered given a last-activity timestamp.
///
/// Returns `RuleOutcome` without executing any actions.
///
/// # Errors
///
/// Returns an error if config group expansion fails.
pub fn evaluate(
    config: &ProjectConfig,
    state: &ProjectState,
    last_modified: DateTime<Utc>,
) -> Result<Vec<RuleOutcome>, FrostxError> {
    let expanded = config.expand_groups()?;
    let mut outcomes = Vec::new();

    for (i, (rule, actions)) in config.rules.iter().zip(expanded.iter()).enumerate() {
        let index = i + 1;
        let triggered = rule.after.has_elapsed_since(last_modified);
        let remaining = rule.after.remaining_seconds_from(last_modified);
        let after_seconds = (Utc::now() - last_modified).num_seconds() - remaining;
        let after_seconds = after_seconds.max(0);

        let rule_hash = rule.rule_hash();
        let action_outcomes = if triggered {
            actions
                .iter()
                .map(|name| {
                    let completed = state.is_completed(&rule_hash, name);
                    ActionOutcome {
                        name: name.clone(),
                        status: if completed {
                            ActionStatus::Completed
                        } else {
                            ActionStatus::Ok
                        },
                        message: if completed {
                            "already completed".into()
                        } else {
                            "pending".into()
                        },
                    }
                })
                .collect()
        } else {
            vec![]
        };

        outcomes.push(RuleOutcome {
            index,
            name: rule.name.clone(),
            after: rule.after.clone(),
            after_seconds,
            triggered,
            remaining_seconds: remaining,
            action_outcomes,
        });
    }

    Ok(outcomes)
}

fn skipped_pipeline_outcomes(
    actions: &[String],
    index: usize,
    rule_name: Option<&str>,
    on_action: &ActionCallback<'_>,
) -> Vec<ActionOutcome> {
    actions
        .iter()
        .map(|action_name| {
            let outcome = ActionOutcome {
                name: action_name.clone(),
                status: ActionStatus::Skipped,
                message: "skipped - preceding rule failed".into(),
            };
            on_action(index, rule_name, &outcome);
            outcome
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn run_rule_actions(
    actions: &[String],
    index: usize,
    rule_name: Option<&str>,
    rule_hash: &str,
    config: &ProjectConfig,
    state: &mut ProjectState,
    current_path: &mut PathBuf,
    dry_run: bool,
    force: bool,
    yes: bool,
    action_filter: Option<&str>,
    on_action: &ActionCallback<'_>,
) -> Result<(Vec<ActionOutcome>, bool), FrostxError> {
    let mut outcomes = Vec::new();
    let mut chain_failed = false;
    for action_name in actions {
        if action_filter.is_some_and(|f| f != action_name) {
            continue;
        }
        if chain_failed {
            let outcome = ActionOutcome {
                name: action_name.clone(),
                status: ActionStatus::Skipped,
                message: "skipped - preceding action failed".into(),
            };
            on_action(index, rule_name, &outcome);
            outcomes.push(outcome);
            continue;
        }
        let action = crate::actions::create(action_name, config)?;
        if action.kind() == crate::actions::ActionKind::Mutation
            && !force
            && state.is_completed(rule_hash, action_name)
        {
            let outcome = ActionOutcome {
                name: action_name.clone(),
                status: ActionStatus::Completed,
                message: "already completed".into(),
            };
            on_action(index, rule_name, &outcome);
            outcomes.push(outcome);
            continue;
        }
        if current_path.is_file() && !action.supports_compressed_archive() {
            let (status, message, fails_chain) =
                if action.kind() == crate::actions::ActionKind::Check {
                    // Check actions are re-evaluated gates; if the project has
                    // already been compressed they are no longer applicable.
                    (
                        ActionStatus::Skipped,
                        format!("'{action_name}' skipped: project is a compressed archive"),
                        false,
                    )
                } else {
                    // A mutation that cannot operate on a compressed archive is a
                    // configuration error — stop the chain.
                    (
                        ActionStatus::Failed,
                        format!(
                            "'{action_name}' cannot run on a compressed archive; \
                             this action requires an uncompressed project directory"
                        ),
                        true,
                    )
                };
            let outcome = ActionOutcome {
                name: action_name.clone(),
                status,
                message,
            };
            on_action(index, rule_name, &outcome);
            outcomes.push(outcome);
            if fails_chain {
                chain_failed = true;
            }
            continue;
        }
        let ctx = crate::actions::ActionContext {
            project_path: current_path.as_path(),
            config,
            dry_run,
            yes,
        };
        let outcome = match action.run(&ctx) {
            Ok(ao) => {
                if !dry_run {
                    if let Some(new_path) = ao.new_project_path {
                        state.project_path.clone_from(&new_path);
                        *current_path = new_path;
                    }
                }
                ActionOutcome {
                    name: action_name.clone(),
                    status: ao.status.clone(),
                    message: ao.message.clone(),
                }
            }
            Err(e) => ActionOutcome {
                name: action_name.clone(),
                status: ActionStatus::Failed,
                message: e.to_string(),
            },
        };
        let failed = outcome.status == ActionStatus::Failed;
        if !dry_run
            && (outcome.status == ActionStatus::Ok || outcome.status == ActionStatus::Completed)
            && action.kind() == crate::actions::ActionKind::Mutation
        {
            state.mark_completed(rule_hash, action_name);
        }
        on_action(index, rule_name, &outcome);
        outcomes.push(outcome);
        if failed {
            chain_failed = true;
        }
    }
    Ok((outcomes, chain_failed))
}

/// Execute the pipeline for a project.
///
/// # Errors
///
/// Returns an error if config group expansion fails or action creation fails.
pub fn run(
    config: &ProjectConfig,
    state: &mut ProjectState,
    project_path: &Path,
    last_modified: DateTime<Utc>,
    opts: &RunOptions,
    on_action: &ActionCallback<'_>,
) -> Result<Vec<RuleOutcome>, FrostxError> {
    let expanded = config.expand_groups()?;
    let mut outcomes = Vec::new();
    let mut pipeline_failed = false;
    // Tracks the live project path; updated in-place when an action relocates
    // the project (e.g. archive.compress replaces the directory with an archive).
    let mut current_path = project_path.to_path_buf();

    for (i, (rule, actions)) in config.rules.iter().zip(expanded.iter()).enumerate() {
        let index = i + 1;
        if let Some(filter) = opts.rule_filter {
            if filter != index {
                continue;
            }
        }
        let triggered = opts.action_filter.is_some() || rule.after.has_elapsed_since(last_modified);
        let remaining = rule.after.remaining_seconds_from(last_modified);
        if !triggered {
            outcomes.push(RuleOutcome {
                index,
                name: rule.name.clone(),
                after: rule.after.clone(),
                after_seconds: 0,
                triggered: false,
                remaining_seconds: remaining,
                action_outcomes: vec![],
            });
            continue;
        }
        if pipeline_failed {
            let action_outcomes =
                skipped_pipeline_outcomes(actions, index, rule.name.as_deref(), on_action);
            outcomes.push(RuleOutcome {
                index,
                name: rule.name.clone(),
                after: rule.after.clone(),
                after_seconds: 0,
                triggered: true,
                remaining_seconds: 0,
                action_outcomes,
            });
            continue;
        }
        let rule_hash = rule.rule_hash();
        let (action_outcomes, chain_failed) = run_rule_actions(
            actions,
            index,
            rule.name.as_deref(),
            &rule_hash,
            config,
            state,
            &mut current_path,
            opts.dry_run,
            opts.force,
            opts.yes,
            opts.action_filter.as_deref(),
            on_action,
        )?;
        if chain_failed {
            pipeline_failed = true;
        }
        outcomes.push(RuleOutcome {
            index,
            name: rule.name.clone(),
            after: rule.after.clone(),
            after_seconds: 0,
            triggered: true,
            remaining_seconds: 0,
            action_outcomes,
        });
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::duration::Duration;
    use crate::config::project::{ActionConfig, Rule};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_config(rules: Vec<Rule>) -> ProjectConfig {
        ProjectConfig {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            include: vec![],
            template: HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules,
        }
    }

    #[test]
    fn untriggered_rule_is_not_triggered() {
        let cfg = make_config(vec![Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["git.check_clean".into()],
        }]);
        let state = ProjectState::default();
        let recent = Utc::now() - chrono::Duration::days(10);
        let outcomes = evaluate(&cfg, &state, recent).unwrap();
        assert!(!outcomes[0].triggered);
    }

    #[test]
    fn triggered_rule_lists_actions() {
        let cfg = make_config(vec![Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["git.check_clean".into()],
        }]);
        let state = ProjectState::default();
        let old = Utc::now() - chrono::Duration::days(100);
        let outcomes = evaluate(&cfg, &state, old).unwrap();
        assert!(outcomes[0].triggered);
        assert_eq!(outcomes[0].action_outcomes.len(), 1);
    }

    #[test]
    fn failed_rule_blocks_subsequent_triggered_rules() {
        use crate::config::project::{HookConfig, HookKind, ProjectConfig};

        let tmp = std::env::temp_dir();
        let mut hooks = HashMap::new();
        hooks.insert(
            "fail_check".into(),
            HookConfig {
                command: "exit 1".into(),
                kind: HookKind::Check,
                run_on_archive: false,
            },
        );
        hooks.insert(
            "should_not_run".into(),
            HookConfig {
                command: "true".into(),
                kind: HookKind::Check,
                run_on_archive: false,
            },
        );
        let cfg = ProjectConfig {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            include: vec![],
            template: HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig {
                hooks,
                ..ActionConfig::default()
            },
            rules: vec![
                Rule {
                    name: None,
                    after: Duration::parse("1h").unwrap(),
                    actions: vec!["hook.fail_check".into()],
                },
                Rule {
                    name: None,
                    after: Duration::parse("1h").unwrap(),
                    actions: vec!["hook.should_not_run".into()],
                },
            ],
        };
        let mut state = ProjectState::default();
        let old = Utc::now() - chrono::Duration::hours(2);
        let opts = RunOptions {
            dry_run: false,
            force: false,
            yes: true,
            rule_filter: None,
            action_filter: None,
        };
        let noop: ActionCallback<'_> = Box::new(|_, _, _| {});
        let outcomes = run(&cfg, &mut state, &tmp, old, &opts, &noop).unwrap();

        assert!(outcomes[0].triggered);
        assert_eq!(outcomes[0].action_outcomes[0].status, ActionStatus::Failed);
        assert!(outcomes[1].triggered);
        assert_eq!(outcomes[1].action_outcomes[0].status, ActionStatus::Skipped);
    }

    #[test]
    fn completed_action_shows_as_completed() {
        let id = Uuid::new_v4();
        let rule = Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["archive.compress".into()],
        };
        let rule_hash = rule.rule_hash();
        let cfg = make_config(vec![rule]);
        let mut state = ProjectState::default();
        state.mark_completed(&rule_hash, "archive.compress");
        let old = Utc::now() - chrono::Duration::days(100);
        let outcomes = evaluate(&cfg, &state, old).unwrap();
        assert_eq!(
            outcomes[0].action_outcomes[0].status,
            ActionStatus::Completed
        );
        let _ = id;
    }
}
