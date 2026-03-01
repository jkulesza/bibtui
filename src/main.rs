mod app;
mod bib;
mod config;
mod search;
mod tui;
mod util;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bibtui", version, about = "A TUI BibTeX manager")]
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

    // Determine bib file path
    let bib_path = cli
        .bib_file
        .or_else(|| config.general.bib_file.clone())
        .context("No .bib file specified. Usage: bibtui <file.bib>")?;

    let bib_path = PathBuf::from(bib_path);
    if !bib_path.exists() {
        anyhow::bail!("File not found: {}", bib_path.display());
    }

    // Create app
    let mut app = app::App::new(bib_path, config)?;

    // Setup terminal
    let mut terminal = tui::setup_terminal()?;

    // Run event loop
    let result = app.run(&mut terminal);

    // Restore terminal (always, even on error)
    tui::restore_terminal(&mut terminal)?;

    result
}
