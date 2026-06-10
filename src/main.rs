use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

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
/// Returns `Ok(None)` when neither is given (open an empty library).
fn resolve_bib_path(cli_arg: Option<String>, config_default: Option<&str>) -> Result<Option<PathBuf>> {
    let path = match cli_arg.or_else(|| config_default.map(String::from)) {
        Some(p) => PathBuf::from(p),
        None => return Ok(None),
    };
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }
    Ok(Some(path))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = config::loader::load_config(cli.config.as_deref())?;

    let mut app = match resolve_bib_path(cli.bib_file, config.general.bib_file.as_deref())? {
        Some(bib_path) => app::App::new(bib_path, config)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
        assert_eq!(resolve_bib_path(None, None).unwrap(), None);
    }

    #[test]
    fn test_resolve_bib_path_cli_arg_beats_config_default() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "@Article{{X,}}").unwrap();
        let cli = tmp.path().to_str().unwrap().to_string();
        let resolved = resolve_bib_path(Some(cli.clone()), Some("/nonexistent/default.bib")).unwrap();
        assert_eq!(resolved, Some(PathBuf::from(cli)));
    }

    #[test]
    fn test_resolve_bib_path_falls_back_to_config_default() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let default = tmp.path().to_str().unwrap();
        let resolved = resolve_bib_path(None, Some(default)).unwrap();
        assert_eq!(resolved, Some(PathBuf::from(default)));
    }

    #[test]
    fn test_resolve_bib_path_missing_file_errors() {
        let err = resolve_bib_path(Some("/nonexistent/refs.bib".to_string()), None).unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }
}
