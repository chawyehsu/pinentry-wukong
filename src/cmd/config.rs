use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use clap::Subcommand;
use miette::IntoDiagnostic;

use crate::config::{self, Config, DEFAULT_TEMPLATE};

/// Manage pinentry-wukong configuration
#[derive(Parser, Debug)]
#[clap(subcommand_required = true)]
pub struct Args {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Open the config file in an editor (creates with defaults if missing)
    Edit,

    /// Display current configuration (merged with defaults)
    List,

    /// Print the config file path
    Path,
}

pub async fn execute(args: Args, config_path: Option<&Path>) -> miette::Result<()> {
    let config_path = config::resolve_config_path(config_path);

    match args.command {
        ConfigCommand::Edit => edit_config(&config_path)?,
        ConfigCommand::List => list_config(&config_path)?,
        ConfigCommand::Path => println!("{}", config_path.display()),
    }

    Ok(())
}

/// Create the config file with defaults if missing, then open in editor.
fn edit_config(path: &PathBuf) -> miette::Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(path, DEFAULT_TEMPLATE).into_diagnostic()?;
    }

    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    let status = Command::new(&editor).arg(path).status().into_diagnostic()?;

    if !status.success() {
        return Err(miette::miette!(
            "editor '{}' exited with status: {}",
            editor,
            status
        ));
    }

    Ok(())
}

/// Display current configuration merged with defaults.
fn list_config(path: &PathBuf) -> miette::Result<()> {
    if !path.exists() {
        print!("{DEFAULT_TEMPLATE}");
        return Ok(());
    }

    let contents = fs::read_to_string(path).into_diagnostic()?;

    // Parse user config to check which keys are set
    let user_value: toml::Value = toml::from_str(&contents).into_diagnostic()?;
    let defaults = Config::default();

    // Build merged output: user values uncommented, missing defaults commented
    let mut output = String::new();

    // [logging] section
    output.push_str("[logging]\n");

    let logging = user_value.get("logging");
    if let Some(enabled) = logging.and_then(|v| v.get("enabled")) {
        output.push_str(&format!("enabled = {}\n", format_toml_value(enabled)));
    } else {
        output.push_str(&format!("# enabled = {}\n", defaults.logging.enabled));
    }

    if let Some(level) = logging.and_then(|v| v.get("level")) {
        output.push_str(&format!("level = {}\n", format_toml_value(level)));
    } else {
        output.push_str(&format!("# level = \"{}\"\n", defaults.logging.level));
    }

    output.push('\n');

    // [general] section
    let general = user_value.get("general");
    let has_general_keys = general
        .and_then(|v| v.as_table())
        .map(|t| !t.is_empty())
        .unwrap_or(false);

    if has_general_keys {
        output.push_str("[general]\n");
    } else {
        output.push_str("# [general]\n");
    }

    if let Some(timeout) = general.and_then(|v| v.get("timeout")) {
        output.push_str(&format!("timeout = {}\n", format_toml_value(timeout)));
    } else {
        output.push_str("# timeout = 60\n");
    }

    if let Some(ui) = general.and_then(|v| v.get("ui")) {
        output.push_str(&format!("ui = {}\n", format_toml_value(ui)));
    } else {
        output.push_str("# ui = \"auto\"\n");
    }

    if let Some(keyring) = general.and_then(|v| v.get("keyring")) {
        output.push_str(&format!("keyring = {}\n", format_toml_value(keyring)));
    } else {
        output.push_str(&format!("# keyring = {}\n", defaults.general.keyring));
    }

    print!("{output}");
    Ok(())
}

/// Format a TOML value for display.
fn format_toml_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => format!("\"{s}\""),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        _ => value.to_string(),
    }
}
