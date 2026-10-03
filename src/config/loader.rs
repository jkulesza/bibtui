use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::defaults::normalize_citekey_templates;
use super::schema::Config;

/// Load configuration with precedence: CLI flag > ./bibtui.yaml > $XDG_CONFIG_HOME/bibtui/config.yaml
pub fn load_config(cli_config: Option<&str>) -> Result<Config> {
    load_config_from(cli_config, Path::new("."), dirs::config_dir().as_deref())
}

fn load_config_from(
    cli_config: Option<&str>,
    current_dir: &Path,
    config_dir: Option<&Path>,
) -> Result<Config> {
    // An explicitly requested config file must exist — silently falling back
    // to the implicit search paths would hide typos in `--config`.
    if let Some(p) = cli_config {
        let path = current_dir.join(p);
        if !path.exists() {
            anyhow::bail!("Config file not found: {}", path.display());
        }
    }

    // Try paths in order of precedence
    let paths_to_try: Vec<PathBuf> = {
        let mut v = Vec::new();

        if let Some(p) = cli_config {
            v.push(current_dir.join(p));
        }

        v.push(current_dir.join("bibtui.yaml"));
        v.push(current_dir.join("bibtui.yml"));

        if let Some(config_dir) = config_dir {
            v.push(config_dir.join("bibtui").join("config.yaml"));
            v.push(config_dir.join("bibtui").join("config.yml"));
        }

        v
    };

    for path in &paths_to_try {
        if path.exists() {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            let mut config: Config = serde_yaml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
            normalize_citekey_templates(&mut config);
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
    fn test_load_config_missing_explicit_path_errors() {
        // An explicitly requested config file that doesn't exist is an error,
        // not a silent fallback to defaults.
        let result = load_config(Some("/nonexistent/__bibtui_test__.yaml"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Config file not found"), "got: {}", msg);
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
        let directory = tempfile::tempdir().unwrap();
        let cfg = load_config_from(None, directory.path(), None).unwrap();
        assert_eq!(
            serde_yaml::to_string(&cfg).unwrap(),
            serde_yaml::to_string(&Config::default()).unwrap()
        );
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
        writeln!(tmp).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_str().unwrap();
        let cfg = load_config(Some(path)).unwrap();
        let default = super::super::schema::Config::default();
        assert_eq!(cfg.general.backup_on_save, default.general.backup_on_save);
    }

    #[test]
    fn test_load_config_empty_citekey_template_replaced_with_default() {
        // Regression: a user's YAML with `inbook: ''` must NOT leave the template
        // empty — it should be filled in with the default fallback so that
        // generate_citekey never receives an empty template string.
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "citekey:\n  templates:\n    inbook: ''").unwrap();
        tmp.flush().unwrap();
        let cfg = load_config(Some(tmp.path().to_str().unwrap())).unwrap();
        let template = cfg
            .citekey
            .templates
            .get("inbook")
            .expect("inbook must exist");
        assert!(
            !template.is_empty(),
            "empty inbook template should be replaced with default"
        );
        assert!(
            template.contains("[year]"),
            "default template should contain [year]"
        );
        assert!(
            template.contains("[auth]"),
            "default template should contain [auth]"
        );
    }

    #[test]
    fn test_load_config_all_standard_types_present_after_normalize() {
        // After loading any config, all standard entry types should have a
        // non-empty template (either configured or filled in from defaults).
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp).unwrap();
        tmp.flush().unwrap();
        let cfg = load_config(Some(tmp.path().to_str().unwrap())).unwrap();
        for type_name in &[
            "article",
            "book",
            "inbook",
            "inproceedings",
            "techreport",
            "phdthesis",
            "mastersthesis",
            "misc",
            "booklet",
            "incollection",
            "manual",
            "proceedings",
            "unpublished",
        ] {
            let t = cfg
                .citekey
                .templates
                .get(*type_name)
                .unwrap_or_else(|| panic!("type '{}' missing from templates", type_name));
            assert!(
                !t.is_empty(),
                "template for '{}' must not be empty",
                type_name
            );
        }
    }

    #[test]
    fn test_load_config_nonexistent_cli_path_errors() {
        // An explicit CLI path that doesn't exist must not fall through to defaults.
        assert!(load_config(Some("/tmp/__definitely_does_not_exist_xyz.yaml")).is_err());
    }
    #[test]
    fn isolated_search_roots_obey_cli_local_and_user_precedence() {
        let local = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        std::fs::create_dir(user.path().join("bibtui")).unwrap();
        std::fs::write(
            user.path().join("bibtui/config.yml"),
            "general:\n  editor: user",
        )
        .unwrap();
        assert_eq!(
            load_config_from(None, local.path(), Some(user.path()))
                .unwrap()
                .general
                .editor,
            "user"
        );
        std::fs::write(local.path().join("bibtui.yml"), "general:\n  editor: local").unwrap();
        assert_eq!(
            load_config_from(None, local.path(), Some(user.path()))
                .unwrap()
                .general
                .editor,
            "local"
        );
        std::fs::write(
            local.path().join("explicit.yaml"),
            "general:\n  editor: explicit",
        )
        .unwrap();
        assert_eq!(
            load_config_from(Some("explicit.yaml"), local.path(), Some(user.path()))
                .unwrap()
                .general
                .editor,
            "explicit"
        );
    }
}
