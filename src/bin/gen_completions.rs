/// Generates shell completion scripts for frostx.
///
/// Usage: `gen_completions` [`OUTPUT_DIR`]
///
/// Writes `frostx.bash`, `_frostx` (zsh), `frostx.fish`, and `frostx.elv`
/// (elvish) to `OUTPUT_DIR`, defaulting to `./completions` when omitted.
use clap::CommandFactory;
use clap_complete::Shell;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let out_dir: PathBuf = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("completions"), PathBuf::from);

    std::fs::create_dir_all(&out_dir)?;

    let mut cmd = frostx::cli::Cli::command();
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Elvish] {
        clap_complete::generate_to(shell, &mut cmd, "frostx", &out_dir)?;
    }
    Ok(())
}
