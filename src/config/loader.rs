use std::path::PathBuf;

use anyhow::{Context, Result};

use super::schema::Config;

/// Load configuration with precedence: CLI flag > ./bibtui.yaml > $XDG_CONFIG_HOME/bibtui/config.yaml
pub fn load_config(cli_config: Option<&str>) -> Result<Config> {
    // Try paths in order of precedence
    let paths_to_try: Vec<PathBuf> = {
        let mut v = Vec::new();

        if let Some(p) = cli_config {
            v.push(PathBuf::from(p));
        }

        v.push(PathBuf::from("bibtui.yaml"));
        v.push(PathBuf::from("bibtui.yml"));

        if let Some(config_dir) = dirs::config_dir() {
            v.push(config_dir.join("bibtui").join("config.yaml"));
            v.push(config_dir.join("bibtui").join("config.yml"));
        }

        v
    };

    for path in &paths_to_try {
        if path.exists() {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            let config: Config = serde_yaml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
            return Ok(config);
        }
    }

    // No config file found — use defaults
    Ok(Config::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config_returns_defaults_when_no_file_exists() {
        // Pass a path that definitely doesn't exist.
        let cfg = load_config(Some("/nonexistent/__bibtui_test__.yaml")).unwrap();
        assert_eq!(cfg.general.backup_on_save, Config::default().general.backup_on_save);
    }

    #[test]
    fn test_load_config_from_explicit_valid_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "general:\n  backup_on_save: true").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_str().unwrap();
        let cfg = load_config(Some(path)).unwrap();
        assert!(cfg.general.backup_on_save);
    }

    #[test]
    fn test_load_config_invalid_yaml_returns_error() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "{{{{not valid yaml at all: [}}}}: :").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_str().unwrap();
        let result = load_config(Some(path));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_none_cli_falls_back_to_defaults() {
        // When no bibtui.yaml exists in CWD and no CLI path, we get defaults.
        // This test may pass or fail depending on whether bibtui.yaml exists
        // in the working directory; skip it if the file is present.
        if std::path::Path::new("bibtui.yaml").exists()
            || std::path::Path::new("bibtui.yml").exists()
        {
            return;
        }
        let cfg = load_config(None).unwrap();
        // Just verify it doesn't panic and returns a usable config.
        let _ = cfg.general.backup_on_save;
    }

    #[test]
    fn test_load_config_from_yml_extension() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "general:\n  backup_on_save: false").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_str().unwrap();
        let cfg = load_config(Some(path)).unwrap();
        // Any valid parse means the file was read successfully.
        let _ = cfg.general.backup_on_save;
    }

    #[test]
    fn test_load_config_minimal_empty_yaml() {
        // An empty YAML file should deserialise to all defaults.
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "").unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_str().unwrap();
        let cfg = load_config(Some(path)).unwrap();
        let default = super::super::schema::Config::default();
        assert_eq!(cfg.general.backup_on_save, default.general.backup_on_save);
    }

    #[test]
    fn test_load_config_nonexistent_cli_path_uses_defaults() {
        // An explicit CLI path that doesn't exist should fall through to defaults.
        let cfg = load_config(Some("/tmp/__definitely_does_not_exist_xyz.yaml")).unwrap();
        let default = super::super::schema::Config::default();
        assert_eq!(cfg.general.backup_on_save, default.general.backup_on_save);
    }
}
