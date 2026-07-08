use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use bibtui::util::path::expand_tilde;
use bibtui::{app, config, tui};

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

/// Resolve which .bib file to open: the CLI argument beats the config default.
/// Returns `None` when neither is given (open an empty library and prompt for
/// a path). A leading `~` is expanded to the home directory. A path that does
/// not exist yet is returned as-is — the app opens a blank library and creates
/// the file on first save.
fn resolve_bib_path(cli_arg: Option<String>, config_default: Option<&str>) -> Option<PathBuf> {
    cli_arg
        .or_else(|| config_default.map(String::from))
        .map(|p| PathBuf::from(expand_tilde(&p)))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = config::loader::load_config(cli.config.as_deref())?;

    let mut app = match resolve_bib_path(cli.bib_file, config.general.bib_file.as_deref()) {
        Some(bib_path) => app::App::new(bib_path, config)?,
        // No file specified — open an empty library and prompt the user for a
        // save path before they can do anything else.
        None => app::App::new_empty(config)?,
    };

    // Restore the terminal if we panic anywhere below, so the user is not left
    // in raw mode on the alternate screen.
    tui::install_panic_hook();

    // Setup terminal
    let mut terminal = tui::setup_terminal()?;

    // Run event loop
    let result = app.run(&mut terminal);

    // Restore terminal (always, even on error)
    tui::restore_terminal(&mut terminal)?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_positional_and_config_flag() {
        let cli = Cli::try_parse_from(["bibtui", "refs.bib", "--config", "c.yaml"]).unwrap();
        assert_eq!(cli.bib_file.as_deref(), Some("refs.bib"));
        assert_eq!(cli.config.as_deref(), Some("c.yaml"));
    }

    #[test]
    fn test_cli_no_args() {
        let cli = Cli::try_parse_from(["bibtui"]).unwrap();
        assert!(cli.bib_file.is_none());
        assert!(cli.config.is_none());
    }

    #[test]
    fn test_resolve_bib_path_none_given() {
        assert_eq!(resolve_bib_path(None, None), None);
    }

    #[test]
    fn test_resolve_bib_path_cli_arg_beats_config_default() {
        let resolved = resolve_bib_path(Some("refs.bib".to_string()), Some("default.bib"));
        assert_eq!(resolved, Some(PathBuf::from("refs.bib")));
    }

    #[test]
    fn test_resolve_bib_path_falls_back_to_config_default() {
        let resolved = resolve_bib_path(None, Some("default.bib"));
        assert_eq!(resolved, Some(PathBuf::from("default.bib")));
    }

    #[test]
    fn test_resolve_bib_path_expands_tilde_in_config_default() {
        if let Some(home) = dirs::home_dir() {
            let resolved = resolve_bib_path(None, Some("~/x.bib"));
            assert_eq!(resolved, Some(home.join("x.bib")));
        }
    }

    #[test]
    fn test_resolve_bib_path_expands_tilde_in_cli_arg() {
        if let Some(home) = dirs::home_dir() {
            let resolved = resolve_bib_path(Some("~/y.bib".to_string()), Some("~/x.bib"));
            assert_eq!(resolved, Some(home.join("y.bib")));
        }
    }

    #[test]
    fn test_resolve_bib_path_missing_file_is_returned_as_is() {
        // A nonexistent path is not an error: the app opens a blank library
        // and creates the file on first save.
        let resolved = resolve_bib_path(Some("/nonexistent/refs.bib".to_string()), None);
        assert_eq!(resolved, Some(PathBuf::from("/nonexistent/refs.bib")));
    }
}
