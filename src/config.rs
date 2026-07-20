use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize};

/// Valid tracing log levels.
const VALID_LEVELS: &[&str] = &["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// Valid UI modes for config.
const VALID_UI_MODES: &[&str] = &["auto", "tty", "tui", "prefer-tty", "prefer-gui"];

/// Default config file template shown by `config edit` and `config list`.
pub const DEFAULT_TEMPLATE: &str = r#"[logging]
enabled = false
# level = "INFO"

# [general]
# timeout = 60
# ui = "auto"
# keyring = true
"#;

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct Config {
    pub logging: LoggingConfig,
    pub general: GeneralConfig,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct LoggingConfig {
    /// Whether logging is enabled.
    pub enabled: bool,
    /// Log level (OFF, ERROR, WARN, INFO, DEBUG, TRACE).
    pub level: String,
}

/// General configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct GeneralConfig {
    /// Input timeout in seconds. None uses the hardcoded default (60).
    pub timeout: Option<u32>,
    /// UI mode override. None uses auto-detection.
    pub ui: Option<String>,
    /// Whether to use the system keyring.
    pub keyring: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: "INFO".to_string(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            timeout: None,
            ui: None,
            keyring: true,
        }
    }
}

impl Config {
    /// Load and validate configuration from a TOML file.
    pub fn load(path: &Path) -> miette::Result<Self> {
        let contents = std::fs::read_to_string(path).into_diagnostic()?;
        let config: Config = toml::from_str(&contents).into_diagnostic()?;
        config.validate()?;
        Ok(config)
    }

    /// Validate config values.
    fn validate(&self) -> miette::Result<()> {
        if !VALID_LEVELS.contains(&self.logging.level.as_str()) {
            return Err(miette::miette!(
                "invalid logging level '{}', must be one of: {}",
                self.logging.level,
                VALID_LEVELS.join(", ")
            ));
        }
        if let Some(ref ui) = self.general.ui
            && !VALID_UI_MODES.contains(&ui.as_str())
        {
            return Err(miette::miette!(
                "invalid ui mode '{}', must be one of: {}",
                ui,
                VALID_UI_MODES.join(", ")
            ));
        }
        Ok(())
    }
}

/// Resolve the config file path.
///
/// Priority: CLI `--config` > `$XDG_CONFIG_HOME/pinentry-wukong/config.toml` > platform default.
pub fn resolve_config_path(cli_path: Option<&Path>) -> PathBuf {
    if let Some(path) = cli_path {
        return path.to_path_buf();
    }

    // Check XDG_CONFIG_HOME env var first (works on all platforms, including Windows)
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg)
            .join("pinentry-wukong")
            .join("config.toml");
    }

    // Fall back to platform-specific default via `directories` crate
    if let Some(proj_dirs) = ProjectDirs::from("", "", "pinentry-wukong") {
        return proj_dirs.config_dir().join("config.toml");
    }

    // Last resort
    PathBuf::from("pinentry-wukong-config.toml")
}

/// Resolve the log file path.
///
/// Priority: `$XDG_CACHE_HOME/pinentry-wukong.log` > platform default.
pub fn resolve_log_path() -> PathBuf {
    // Check XDG_CACHE_HOME env var first (works on all platforms, including Windows)
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("pinentry-wukong.log");
    }

    // Fall back to platform-specific default via `directories` crate
    if let Some(proj_dirs) = ProjectDirs::from("", "", "pinentry-wukong") {
        return proj_dirs.cache_dir().join("pinentry-wukong.log");
    }

    // Last resort
    PathBuf::from("pinentry-wukong.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.logging.enabled);
        assert_eq!(config.logging.level, "INFO");
        assert_eq!(config.general.timeout, None);
        assert_eq!(config.general.ui, None);
        assert!(config.general.keyring);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
[logging]
enabled = true
level = "DEBUG"

[general]
timeout = 30
ui = "tui"
keyring = false
"#;
        let config: Config = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.logging.enabled);
        assert_eq!(config.logging.level, "DEBUG");
        assert_eq!(config.general.timeout, Some(30));
        assert_eq!(config.general.ui.as_deref(), Some("tui"));
        assert!(!config.general.keyring);
    }

    #[test]
    fn test_parse_partial_config() {
        let toml = r#"
[logging]
enabled = true
"#;
        let config: Config = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.logging.enabled);
        assert_eq!(config.logging.level, "INFO"); // default
        assert_eq!(config.general.timeout, None); // default
        assert!(config.general.keyring); // default
    }

    #[test]
    fn test_invalid_level() {
        let toml = r#"
[logging]
level = "VERBOSE"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_ui_mode() {
        let toml = r#"
[general]
ui = "gui"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_unknown_key_rejected() {
        let toml = r#"
[logging]
enabled = true
unknown_key = "value"
"#;
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_section_rejected() {
        let toml = r#"
[unknown]
key = "value"
"#;
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }
}
