use crate::config::duration::Duration;
use crate::config::project::ProjectConfig;
use crate::config::state::ProjectState;
use crate::error::FrostxError;
use chrono::DateTime;
use chrono::Utc;
use serde::Serialize;
use std::path::Path;

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
pub type ActionCallback<'a> = Box<dyn Fn(usize, &ActionOutcome) + 'a>;

/// Evaluate which rules are triggered given a last-activity timestamp.
/// Returns `RuleOutcome` without executing any actions.
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

        let action_outcomes = if triggered {
            actions
                .iter()
                .map(|name| {
                    let completed = state.is_completed(index, name);
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
            after: rule.after.clone(),
            after_seconds,
            triggered,
            remaining_seconds: remaining,
            action_outcomes,
        });
    }

    Ok(outcomes)
}

/// Execute the pipeline for a project.
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
    let ctx_yes = opts.yes;
    let ctx_dry_run = opts.dry_run;
    let ctx_force = opts.force; // used for skipping completed-action check
    let mut pipeline_failed = false;

    for (i, (rule, actions)) in config.rules.iter().zip(expanded.iter()).enumerate() {
        let index = i + 1;

        // Rule filter: skip rules not matching the filter.
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
                after: rule.after.clone(),
                after_seconds: 0,
                triggered: false,
                remaining_seconds: remaining,
                action_outcomes: vec![],
            });
            continue;
        }

        // A failed rule blocks all subsequent rules for this run.
        if pipeline_failed {
            let action_outcomes = actions
                .iter()
                .map(|action_name| {
                    let outcome = ActionOutcome {
                        name: action_name.clone(),
                        status: ActionStatus::Skipped,
                        message: "skipped - preceding rule failed".into(),
                    };
                    on_action(index, &outcome);
                    outcome
                })
                .collect();
            outcomes.push(RuleOutcome {
                index,
                after: rule.after.clone(),
                after_seconds: 0,
                triggered: true,
                remaining_seconds: 0,
                action_outcomes,
            });
            continue;
        }

        let mut action_outcomes = Vec::new();
        let mut chain_failed = false;

        for action_name in actions {
            // Single-action filter.
            if let Some(ref filter) = opts.action_filter {
                if filter != action_name {
                    continue;
                }
            }

            if chain_failed {
                let outcome = ActionOutcome {
                    name: action_name.clone(),
                    status: ActionStatus::Skipped,
                    message: "skipped - preceding action failed".into(),
                };
                on_action(index, &outcome);
                action_outcomes.push(outcome);
                continue;
            }

            // Check if a mutation was already completed.
            let action = crate::actions::create(action_name, config)?;
            if action.kind() == crate::actions::ActionKind::Mutation
                && !ctx_force
                && state.is_completed(index, action_name)
            {
                let outcome = ActionOutcome {
                    name: action_name.clone(),
                    status: ActionStatus::Completed,
                    message: "already completed".into(),
                };
                on_action(index, &outcome);
                action_outcomes.push(outcome);
                continue;
            }

            let ctx = crate::actions::ActionContext {
                project_path,
                config,
                dry_run: ctx_dry_run,
                yes: ctx_yes,
            };

            let action_result = action.run(&ctx);

            let outcome = match action_result {
                Ok(ao) => ActionOutcome {
                    name: action_name.clone(),
                    status: ao.status.clone(),
                    message: ao.message.clone(),
                },
                Err(e) => ActionOutcome {
                    name: action_name.clone(),
                    status: ActionStatus::Failed,
                    message: e.to_string(),
                },
            };

            let failed = outcome.status == ActionStatus::Failed;

            // Record completed mutations.
            if !ctx_dry_run
                && (outcome.status == ActionStatus::Ok || outcome.status == ActionStatus::Completed)
                && action.kind() == crate::actions::ActionKind::Mutation
            {
                state.mark_completed(index, action_name);
            }

            on_action(index, &outcome);
            action_outcomes.push(outcome);

            if failed {
                chain_failed = true;
            }
        }

        if chain_failed {
            pipeline_failed = true;
        }

        outcomes.push(RuleOutcome {
            index,
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
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules,
        }
    }

    #[test]
    fn untriggered_rule_is_not_triggered() {
        let cfg = make_config(vec![Rule {
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
            },
        );
        hooks.insert(
            "should_not_run".into(),
            HookConfig {
                command: "true".into(),
                kind: HookKind::Check,
            },
        );
        let cfg = ProjectConfig {
            id: Uuid::new_v4(),
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig {
                hooks,
                ..ActionConfig::default()
            },
            rules: vec![
                Rule {
                    after: Duration::parse("1h").unwrap(),
                    actions: vec!["hook.fail_check".into()],
                },
                Rule {
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
        let noop: ActionCallback<'_> = Box::new(|_, _| {});
        let outcomes = run(&cfg, &mut state, &tmp, old, &opts, &noop).unwrap();

        assert!(outcomes[0].triggered);
        assert_eq!(outcomes[0].action_outcomes[0].status, ActionStatus::Failed);
        assert!(outcomes[1].triggered);
        assert_eq!(outcomes[1].action_outcomes[0].status, ActionStatus::Skipped);
    }

    #[test]
    fn completed_action_shows_as_completed() {
        let id = Uuid::new_v4();
        let cfg = make_config(vec![Rule {
            after: Duration::parse("90d").unwrap(),
            actions: vec!["archive.tar_gz".into()],
        }]);
        let mut state = ProjectState::default();
        state.mark_completed(1, "archive.tar_gz");
        let old = Utc::now() - chrono::Duration::days(100);
        let outcomes = evaluate(&cfg, &state, old).unwrap();
        assert_eq!(
            outcomes[0].action_outcomes[0].status,
            ActionStatus::Completed
        );
        let _ = id;
    }
}
