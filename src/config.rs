use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "rewatch",
    about = "File watch and restart tool — a smarter cargo-watch alternative for AI coding agents",
    after_help = "\
Examples:
  rewatch -w src,Cargo.toml -e rs,toml -- cargo run
  rewatch -w src -w tests -e rs -- cargo test
  rewatch -t .rewatch-trigger -w src -e rs -- cargo run

Config file (rewatch.toml):
  command = \"cargo run\"
  watch = [\"src\", \"Cargo.toml\"]
  ext = [\"rs\", \"toml\"]
  trigger = \".rewatch-trigger\"

  [env]
  RUST_LOG = \"debug\"

Run without arguments to use rewatch.toml from the current directory."
)]
pub struct CliArgs {
    /// Paths to watch (comma-separated or multiple -w flags)
    #[arg(short, long, value_delimiter = ',')]
    pub watch: Vec<String>,

    /// File extensions to filter (comma-separated or multiple -e flags)
    #[arg(short, long, value_delimiter = ',')]
    pub ext: Vec<String>,

    /// Trigger file — auto-restart without Enter when this file changes
    #[arg(short, long)]
    pub trigger: Option<String>,

    /// Trigger restarts even without file changes (default: only when waiting)
    #[arg(short = 'T', long = "trigger-always")]
    pub trigger_always: bool,

    /// Environment variables (KEY=VALUE, can be repeated)
    #[arg(short = 'E', long = "env", value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// Path to config file (default: rewatch.toml in cwd)
    #[arg(short = 'c', long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Command to run (everything after --)
    #[arg(last = true)]
    pub command: Vec<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct FileConfig {
    pub command: Option<String>,
    pub watch: Option<Vec<String>>,
    pub ext: Option<Vec<String>>,
    pub trigger: Option<String>,
    pub trigger_always: Option<bool>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug)]
pub struct Config {
    pub command: Vec<String>,
    pub watch: Vec<PathBuf>,
    pub ext: Vec<String>,
    pub trigger: Option<PathBuf>,
    pub trigger_always: bool,
    pub env: HashMap<String, String>,
    /// Path to the loaded config file. `None` when no `--config` was given
    /// and the default `rewatch.toml` was not present (CLI-only mode).
    pub config_path: Option<PathBuf>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let cli = CliArgs::parse();
        let (candidate_path, required) = match cli.config.clone() {
            Some(p) => (p, true),
            None => (PathBuf::from("rewatch.toml"), false),
        };
        let file_config = load_file_config(&candidate_path, required)?;
        // Only set config_path when we actually have a file to watch.
        // For CLI-only mode (no --config, no rewatch.toml present) this is None.
        let config_path = file_config.as_ref().map(|_| candidate_path);
        let source_label = config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "rewatch.toml".to_string());

        // Command: CLI has priority, then TOML
        let command = if !cli.command.is_empty() {
            cli.command
        } else if let Some(ref fc) = file_config {
            if let Some(ref cmd) = fc.command {
                shlex::split(cmd).ok_or_else(|| format!("Invalid command in {source_label}: unclosed quote in '{cmd}'"))?
            } else {
                return Err(format!("No command specified. Use -- <command> or set 'command' in {source_label}"));
            }
        } else {
            return Err(format!("No command specified. Use -- <command> or set 'command' in {source_label}"));
        };

        // Watch paths: CLI has priority
        let watch: Vec<PathBuf> = if !cli.watch.is_empty() {
            cli.watch.into_iter().map(PathBuf::from).collect()
        } else if let Some(ref fc) = file_config {
            fc.watch.clone().unwrap_or_default().into_iter().map(PathBuf::from).collect()
        } else {
            vec![]
        };

        if watch.is_empty() {
            return Err(format!("No watch paths specified. Use -w <paths> or set 'watch' in {source_label}"));
        }

        // Extensions: CLI has priority. Normalize: strip leading dot (.rs → rs)
        let ext: Vec<String> = if !cli.ext.is_empty() {
            cli.ext
        } else if let Some(ref fc) = file_config {
            fc.ext.clone().unwrap_or_default()
        } else {
            vec![]
        }
        .into_iter()
        .map(|e| e.strip_prefix('.').unwrap_or(&e).to_string())
        .collect();

        // Trigger: CLI has priority
        let trigger = cli.trigger
            .or_else(|| file_config.as_ref().and_then(|fc| fc.trigger.clone()))
            .map(PathBuf::from);

        // Trigger always: CLI flag wins, then TOML, default false
        let trigger_always = cli.trigger_always
            || file_config.as_ref().and_then(|fc| fc.trigger_always).unwrap_or(false);

        // Env: TOML as base, CLI overrides
        let mut env = file_config
            .and_then(|fc| fc.env)
            .unwrap_or_default();
        for item in &cli.env {
            if let Some((key, value)) = item.split_once('=') {
                env.insert(key.to_string(), value.to_string());
            } else {
                return Err(format!("Invalid env format: '{item}'. Expected KEY=VALUE"));
            }
        }

        Ok(Config { command, watch, ext, trigger, trigger_always, env, config_path })
    }
}

/// Load a TOML config file.
/// - File absent + `required` → error (user explicitly asked for it via `--config`).
/// - File absent + not required → `Ok(None)` (default `rewatch.toml`, CLI-only mode).
/// - File present but unreadable/unparseable → always error (file exists, user has a bug to fix).
fn load_file_config(path: &Path, required: bool) -> Result<Option<FileConfig>, String> {
    if !path.exists() {
        if required {
            return Err(format!("Config file not found: {}", path.display()));
        }
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    toml::from_str::<FileConfig>(&content)
        .map(Some)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

