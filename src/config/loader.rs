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
