mod app;
mod bib;
mod config;
mod search;
mod tui;
mod util;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bibtui", version = env!("GIT_VERSION"), about = "A TUI BibTeX manager")]
struct Cli {
    /// Path to .bib file
    #[arg()]
    bib_file: Option<String>,

    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = config::loader::load_config(cli.config.as_deref())?;

    // Determine bib file path (CLI arg beats config default)
    let bib_path_opt = cli
        .bib_file
        .or_else(|| config.general.bib_file.clone());

    let mut app = match bib_path_opt {
        Some(path) => {
            let bib_path = PathBuf::from(&path);
            if !bib_path.exists() {
                anyhow::bail!("File not found: {}", bib_path.display());
            }
            app::App::new(bib_path, config)?
        }
        // No file specified — open an empty library and prompt the user for a
        // save path before they can do anything else.
        None => app::App::new_empty(config)?,
    };

    // Setup terminal
    let mut terminal = tui::setup_terminal()?;

    // Run event loop
    let result = app.run(&mut terminal);

    // Restore terminal (always, even on error)
    tui::restore_terminal(&mut terminal)?;

    result
}
