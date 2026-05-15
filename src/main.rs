use clap::Parser;
use directories::ProjectDirs;
use frostx::cli::{Cli, Cmd, ProjectsCmd};
use frostx::error::{exit_code, FrostxError};
use frostx::ops::{
    self,
    init::InitArgs,
    projects::{ProjectsAddArgs, ProjectsRunArgs},
    run::RunArgs,
    scan::ScanArgs,
    FrostxOpts,
};
use frostx::output::{
    human, json, DailyCheckOutput, DailyRunOutput, DailySource, OutputFormat, RunActionOutput,
    FROSTX_VERSION,
};
use frostx::prompt;
use std::path::PathBuf;
use std::process;

#[allow(clippy::too_many_lines)]
fn main() {
    let cli = Cli::parse();

    let dirs = ProjectDirs::from("", "", "frostx");

    let library_dir = cli.library.unwrap_or_else(|| {
        dirs.as_ref().map_or_else(
            || PathBuf::from(".frostx-library"),
            |d| d.config_dir().join("library"),
        )
    });

    let state_dir = cli.state_dir.unwrap_or_else(|| {
        dirs.as_ref().map_or_else(
            || PathBuf::from(".frostx-state"),
            |d| d.data_dir().to_path_buf(),
        )
    });

    let format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };

    let pretend_inactive = match cli
        .pretend_inactive
        .as_deref()
        .map(frostx::config::duration::Duration::parse)
    {
        Some(Ok(d)) => Some(d),
        Some(Err(e)) => {
            emit_error_msg(
                &format!("--pretend-inactive: {e}"),
                frostx::error::exit_code::ERROR,
                format,
            );
            process::exit(frostx::error::exit_code::ERROR);
        }
        None => None,
    };

    let opts = FrostxOpts {
        dry_run: cli.dry_run,
        verbose: cli.verbose > 0,
        quiet: cli.quiet,
        yes: cli.yes,
        config_override: cli.config,
        library_dir,
        state_dir,
        pretend_inactive,
    };

    let code = match cli.command {
        Cmd::Init {
            path,
            include,
            force,
        } => {
            // Run the interactive questionnaire when stdin is a TTY and --yes
            // was not passed. --json implies non-interactive (machine-readable).
            let interactive = !opts.yes && !cli.json;
            let questionnaire =
                prompt::run_init_questionnaire(&include, &opts.library_dir, interactive);
            let (init_name, init_description, init_includes, init_template) = match questionnaire {
                Ok(Some(q)) => (q.name, q.description, q.includes, q.template),
                Ok(None) => (None, None, include, std::collections::HashMap::new()),
                Err(e) => {
                    emit_error_msg(
                        &format!("questionnaire error: {e}"),
                        exit_code::ERROR,
                        format,
                    );
                    process::exit(exit_code::ERROR);
                }
            };
            let args = InitArgs {
                path,
                includes: init_includes,
                name: init_name,
                description: init_description,
                template: init_template,
                force,
            };
            match ops::init::execute(&args, &opts) {
                Ok(out) => {
                    match format {
                        OutputFormat::Human => human::print_init(&out),
                        OutputFormat::Json => json::print_init(&out),
                    }
                    exit_code::OK
                }
                Err(e) => emit_error(&e, format),
            }
        }

        Cmd::Check { path } => match ops::check::gather(&path, &opts) {
            Ok(out) => {
                match format {
                    OutputFormat::Human => human::print_check(&out),
                    OutputFormat::Json => json::print_check(&out),
                }
                exit_code::OK
            }
            Err(e) => emit_error(&e, format),
        },

        Cmd::Run {
            path,
            rule,
            action,
            force,
        } => {
            let run_args = RunArgs {
                path,
                rule_filter: rule,
                action_filter: action,
                force,
            };
            let cb: frostx::pipeline::ActionCallback<'_> = Box::new(
                move |rule_idx, rule_name, ao: &frostx::pipeline::ActionOutcome| {
                    let out = RunActionOutput {
                        frostx_version: FROSTX_VERSION,
                        project: None,
                        rule: rule_idx,
                        rule_name: rule_name.map(str::to_owned),
                        action: ao.name.clone(),
                        status: ao.status.as_str().to_string(),
                        message: ao.message.clone(),
                    };
                    match format {
                        OutputFormat::Human => human::print_run_action(&out),
                        OutputFormat::Json => json::print_run_action(&out),
                    }
                },
            );
            match ops::run::execute(&run_args, &opts, &cb) {
                Ok(had_failures) => {
                    if had_failures {
                        exit_code::ERROR
                    } else {
                        exit_code::OK
                    }
                }
                Err(e) => emit_error(&e, format),
            }
        }

        Cmd::Scan {
            root,
            triggered_only,
            depth,
        } => {
            let args = ScanArgs {
                root,
                triggered_only,
                depth,
            };
            match ops::scan::execute(&args, &opts) {
                Ok(results) => {
                    match format {
                        OutputFormat::Human => {
                            for out in &results {
                                human::print_check(out);
                            }
                        }
                        OutputFormat::Json => json::print_scan(&results),
                    }
                    exit_code::OK
                }
                Err(e) => emit_error(&e, format),
            }
        }

        Cmd::Doctor { path } => match ops::doctor::execute(&path, &opts) {
            Ok(out) => {
                match format {
                    OutputFormat::Human => human::print_doctor(&out),
                    OutputFormat::Json => json::print_doctor(&out),
                }
                if !out.errors.is_empty() {
                    exit_code::ERROR
                } else if !out.warnings.is_empty() {
                    exit_code::WARNING
                } else {
                    exit_code::OK
                }
            }
            Err(e) => emit_error(&e, format),
        },

        Cmd::Gc => match ops::gc::execute(opts.dry_run, &opts) {
            Ok(out) => {
                match format {
                    OutputFormat::Human => human::print_gc(&out),
                    OutputFormat::Json => json::print_gc(&out),
                }
                exit_code::OK
            }
            Err(e) => emit_error(&e, format),
        },

        Cmd::Projects { subcmd } => match subcmd {
            ProjectsCmd::List => match ops::projects::list(&opts) {
                Ok(out) => {
                    match format {
                        OutputFormat::Human => human::print_projects_list(&out),
                        OutputFormat::Json => json::print_projects_list(&out),
                    }
                    exit_code::OK
                }
                Err(e) => emit_error(&e, format),
            },

            ProjectsCmd::Add { paths, scan } => {
                if paths.is_empty() && scan.is_none() {
                    emit_error_msg(
                        "no projects specified; pass a PATH or --scan <DIR>",
                        exit_code::ERROR,
                        format,
                    );
                    exit_code::ERROR
                } else {
                    let args = ProjectsAddArgs {
                        paths,
                        scan_dir: scan,
                    };
                    let out = ops::projects::add(&args, &opts);
                    let had_skips = !out.skipped.is_empty();
                    if !out.added.is_empty() {
                        daily_reset(&opts);
                    }
                    match format {
                        OutputFormat::Human => human::print_projects_add(&out),
                        OutputFormat::Json => json::print_projects_add(&out),
                    }
                    if had_skips {
                        exit_code::ERROR
                    } else {
                        exit_code::OK
                    }
                }
            }

            ProjectsCmd::Rm { path } => match ops::projects::rm(&path, &opts) {
                Ok(out) => {
                    daily_reset(&opts);
                    match format {
                        OutputFormat::Human => human::print_projects_rm(&out),
                        OutputFormat::Json => json::print_projects_rm(&out),
                    }
                    exit_code::OK
                }
                Err(e) => emit_error(&e, format),
            },

            ProjectsCmd::Check { daily } => {
                if daily {
                    let daily_state = daily_load(&opts);
                    if !is_interactive_terminal() || daily_state.checked_today() {
                        if format == OutputFormat::Json {
                            if let Some(cached) = &daily_state.check_output_json {
                                // Cache stores the full envelope JSON — print verbatim.
                                println!("{cached}");
                            } else {
                                json::print_daily_check(&DailyCheckOutput {
                                    frostx_version: FROSTX_VERSION,
                                    daily_source: DailySource::NotRun,
                                    results: &[],
                                });
                            }
                        }
                        process::exit(exit_code::OK);
                    }
                }
                let (results, errors) = ops::projects::check_all(&opts);
                if daily {
                    if format == OutputFormat::Json {
                        // Store the envelope (with "cached" source) so replay is verbatim.
                        let cache = serde_json::to_string(&DailyCheckOutput {
                            frostx_version: FROSTX_VERSION,
                            daily_source: DailySource::Cached,
                            results: &results,
                        })
                        .ok();
                        daily_record_check(&opts, cache);
                    } else {
                        daily_record_check(&opts, None);
                    }
                }
                let mut worst = exit_code::OK;
                match format {
                    OutputFormat::Human => {
                        for out in &results {
                            human::print_check(out);
                        }
                        for (path, e) in &errors {
                            human::print_error(&format!("{}: {}", path.display(), e));
                            if worst == exit_code::OK {
                                worst = e.exit_code();
                            }
                        }
                    }
                    OutputFormat::Json => {
                        if daily {
                            json::print_daily_check(&DailyCheckOutput {
                                frostx_version: FROSTX_VERSION,
                                daily_source: DailySource::Fresh,
                                results: &results,
                            });
                        } else {
                            json::print_scan(&results);
                        }
                        for (_, e) in &errors {
                            if worst == exit_code::OK {
                                worst = e.exit_code();
                            }
                        }
                    }
                }
                worst
            }

            ProjectsCmd::Run {
                force,
                rule,
                action,
                daily,
            } => {
                if daily {
                    let daily_state = daily_load(&opts);
                    if !is_interactive_terminal() || daily_state.ran_today() {
                        if format == OutputFormat::Json {
                            if let Some(cached) = &daily_state.run_output_json {
                                // Cache stores the full envelope JSON — print verbatim.
                                println!("{cached}");
                            } else {
                                json::print_daily_run(&DailyRunOutput {
                                    frostx_version: FROSTX_VERSION,
                                    daily_source: DailySource::NotRun,
                                    actions: &[],
                                });
                            }
                        }
                        process::exit(exit_code::OK);
                    }
                }
                let run_args = ProjectsRunArgs {
                    force,
                    rule_filter: rule,
                    action_filter: action,
                };
                // When --daily --json: buffer actions for the envelope instead of streaming.
                let collected: std::cell::RefCell<Vec<RunActionOutput>> =
                    std::cell::RefCell::new(Vec::new());
                let (had_failures, errors) = ops::projects::run_all(
                    &run_args,
                    &opts,
                    &|project_path, rule_idx, rule_name, ao| {
                        let out = RunActionOutput {
                            frostx_version: FROSTX_VERSION,
                            project: Some(project_path.display().to_string()),
                            rule: rule_idx,
                            rule_name: rule_name.map(str::to_owned),
                            action: ao.name.clone(),
                            status: ao.status.as_str().to_string(),
                            message: ao.message.clone(),
                        };
                        if daily && format == OutputFormat::Json {
                            collected.borrow_mut().push(out);
                        } else {
                            match format {
                                OutputFormat::Human => human::print_run_action(&out),
                                OutputFormat::Json => json::print_run_action(&out),
                            }
                        }
                    },
                );
                let actions = collected.into_inner();
                if daily {
                    if format == OutputFormat::Json {
                        // Emit fresh envelope, then store cached envelope for replay.
                        json::print_daily_run(&DailyRunOutput {
                            frostx_version: FROSTX_VERSION,
                            daily_source: DailySource::Fresh,
                            actions: &actions,
                        });
                        let cache = serde_json::to_string(&DailyRunOutput {
                            frostx_version: FROSTX_VERSION,
                            daily_source: DailySource::Cached,
                            actions: &actions,
                        })
                        .ok();
                        daily_record_run(&opts, cache);
                    } else {
                        daily_record_run(&opts, None);
                    }
                }
                let mut worst = if had_failures {
                    exit_code::ERROR
                } else {
                    exit_code::OK
                };
                for (path, e) in &errors {
                    let msg = format!("{}: {}", path.display(), e);
                    match format {
                        OutputFormat::Human => human::print_error(&msg),
                        OutputFormat::Json => json::print_error(&msg, e.exit_code()),
                    }
                    if worst == exit_code::OK {
                        worst = e.exit_code();
                    }
                }
                worst
            }
        },
    };

    process::exit(code);
}

fn emit_error(e: &FrostxError, format: OutputFormat) -> i32 {
    match format {
        OutputFormat::Human => human::print_error(&e.to_string()),
        OutputFormat::Json => json::print_error(&e.to_string(), e.exit_code()),
    }
    e.exit_code()
}

fn emit_error_msg(msg: &str, code: i32, format: OutputFormat) {
    match format {
        OutputFormat::Human => human::print_error(msg),
        OutputFormat::Json => json::print_error(msg, code),
    }
}

/// Returns `true` when stdout is an interactive terminal.
fn is_interactive_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Load the daily state from the state directory, returning a fresh default on error.
fn daily_load(opts: &FrostxOpts) -> frostx::config::daily::DailyState {
    frostx::config::daily::DailyState::load(&opts.state_dir).unwrap_or_default()
}

/// Persist the run timestamp and optional NDJSON cache for `projects run --daily`.
fn daily_record_run(opts: &FrostxOpts, ndjson: Option<String>) {
    let mut state = daily_load(opts);
    state.record_run(ndjson);
    let _ = state.save(&opts.state_dir);
}

/// Persist the check timestamp and optional JSON cache for `projects check --daily`.
fn daily_record_check(opts: &FrostxOpts, json: Option<String>) {
    let mut state = daily_load(opts);
    state.record_check(json);
    let _ = state.save(&opts.state_dir);
}

/// Erase the daily cache so the next `--daily` invocation runs fresh.
///
/// Called whenever the tracked-project list changes (`projects add` / `projects rm`).
fn daily_reset(opts: &FrostxOpts) {
    let _ = frostx::config::daily::DailyState::default().save(&opts.state_dir);
}
