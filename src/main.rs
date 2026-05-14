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
use frostx::output::{human, json, OutputFormat, RunActionOutput, FROSTX_VERSION};
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

    let opts = FrostxOpts {
        dry_run: cli.dry_run,
        verbose: cli.verbose > 0,
        quiet: cli.quiet,
        yes: cli.yes,
        config_override: cli.config,
        library_dir,
        state_dir,
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
                    match format {
                        OutputFormat::Human => human::print_projects_rm(&out),
                        OutputFormat::Json => json::print_projects_rm(&out),
                    }
                    exit_code::OK
                }
                Err(e) => emit_error(&e, format),
            },

            ProjectsCmd::Check => {
                let (results, errors) = ops::projects::check_all(&opts);
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
                        json::print_scan(&results);
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
            } => {
                let run_args = ProjectsRunArgs {
                    force,
                    rule_filter: rule,
                    action_filter: action,
                };
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
                        match format {
                            OutputFormat::Human => human::print_run_action(&out),
                            OutputFormat::Json => json::print_run_action(&out),
                        }
                    },
                );
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
