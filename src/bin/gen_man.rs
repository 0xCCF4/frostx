/// Generates man pages for all frostx subcommands using `clap_mangen`.
///
/// Usage: `gen_man` [`OUTPUT_DIR`]
///
/// Writes section-1 man pages (`frostx.1`, `frostx-init.1`, ...) to `OUTPUT_DIR`,
/// defaulting to `./man` when omitted.
use clap::CommandFactory;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let out_dir: PathBuf = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("man"), PathBuf::from);

    std::fs::create_dir_all(&out_dir)?;
    clap_mangen::generate_to(frostx::cli::Cli::command(), &out_dir)?;
    Ok(())
}
