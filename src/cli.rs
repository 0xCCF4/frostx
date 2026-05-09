use clap::{Parser, Subcommand};
use std::path::PathBuf;

const EXTRA_HELP: &'static str =
    "frostx.toml(5), frostx-actions(5), frostx-includes(5), frostx-state(5)";

/// frostx - project lifecycle manager.
///
/// Scans project folders for inactivity and executes configured action
/// pipelines: checking git state, archiving, uploading to a backup server,
/// and cleaning up local copies.
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug)]
#[command(name = "frostx", version, about, long_about = None, after_help = EXTRA_HELP)]
pub struct Cli {
    /// Output all results as JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    /// Show what would happen without executing any actions.
    #[arg(short = 'n', long, global = true)]
    pub dry_run: bool,

    /// Increase output verbosity (-v or -vv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Skip interactive confirmations (local.delete always confirms regardless).
    #[arg(short, long, global = true)]
    pub yes: bool,

    /// Override the frostx.toml path.
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Override the config library directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub library: Option<PathBuf>,

    /// Override the state directory (default: `$XDG_DATA_HOME/frostx/`).
    #[arg(long, global = true, value_name = "DIR")]
    pub state_dir: Option<PathBuf>,

    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Cmd,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Initialize a new frostx project by creating frostx.toml.
    Init {
        /// Directory to initialize (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Bootstrap from a library template (repeatable).
        #[arg(long, value_name = "NAME")]
        include: Vec<String>,

        /// Overwrite an existing frostx.toml and assign a new UUID.
        #[arg(long)]
        force: bool,
    },

    /// Report inactivity and rule trigger status without running actions.
    Check {
        /// Project directory to check (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Execute the inactivity pipeline and perform triggered actions.
    Run {
        /// Project directory to run (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Run only this rule number (1-indexed).
        #[arg(long, value_name = "N")]
        rule: Option<usize>,

        /// Run a single named action, skipping threshold checks.
        #[arg(long, value_name = "NAME")]
        action: Option<String>,

        /// Re-execute completed mutation actions.
        #[arg(long)]
        force: bool,
    },

    /// Walk a directory tree and report all managed projects.
    Scan {
        /// Root directory to scan (default: current directory).
        #[arg(default_value = ".")]
        root: PathBuf,

        /// Only show projects with at least one triggered rule.
        #[arg(long)]
        triggered_only: bool,

        /// Maximum directory depth (default: unlimited).
        #[arg(long, value_name = "N")]
        depth: Option<usize>,
    },

    /// Validate frostx.toml without running anything.
    Doctor {
        /// Project directory to validate (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Remove orphaned state files with no matching project.
    Gc,

    /// Manage the tracked-project registry.
    Projects {
        /// Projects subcommand.
        #[command(subcommand)]
        subcmd: ProjectsCmd,
    },
}

/// Subcommands for `frostx projects`.
#[derive(Subcommand, Debug)]
pub enum ProjectsCmd {
    /// List all currently tracked projects.
    List,

    /// Register one or more projects in the state directory.
    Add {
        /// Project directories to register (may be repeated).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Recursively scan DIR and register every frostx project found.
        #[arg(long, value_name = "DIR")]
        scan: Option<PathBuf>,
    },

    /// Unregister a project (delete its state file).
    Rm {
        /// Project directory to unregister.
        path: PathBuf,
    },

    /// Run `check` on every tracked project.
    Check,

    /// Run the inactivity pipeline for every tracked project.
    Run {
        /// Re-execute completed mutation actions.
        #[arg(long)]
        force: bool,

        /// Run only rule number N (1-indexed).
        #[arg(long, value_name = "N")]
        rule: Option<usize>,

        /// Run a single named action, skipping threshold checks.
        #[arg(long, value_name = "NAME")]
        action: Option<String>,
    },
}
