use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rewatch", about = "Watch files and restart commands on changes")]
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
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug)]
pub struct Config {
    pub command: Vec<String>,
    pub watch: Vec<PathBuf>,
    pub ext: Vec<String>,
    pub trigger: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let cli = CliArgs::parse();
        let file_config = Self::load_file_config();

        // Command: CLI has priority, then TOML
        let command = if !cli.command.is_empty() {
            cli.command
        } else if let Some(ref fc) = file_config {
            if let Some(ref cmd) = fc.command {
                shlex::split(cmd).ok_or_else(|| format!("Invalid command in rewatch.toml: unclosed quote in '{cmd}'"))?
            } else {
                return Err("No command specified. Use -- <command> or set 'command' in rewatch.toml".into());
            }
        } else {
            return Err("No command specified. Use -- <command> or set 'command' in rewatch.toml".into());
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
            return Err("No watch paths specified. Use -w <paths> or set 'watch' in rewatch.toml".into());
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

        // Env: only from TOML
        let env = file_config
            .and_then(|fc| fc.env)
            .unwrap_or_default();

        Ok(Config { command, watch, ext, trigger, env })
    }

    fn load_file_config() -> Option<FileConfig> {
        let path = PathBuf::from("rewatch.toml");
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        match toml::from_str::<FileConfig>(&content) {
            Ok(fc) => Some(fc),
            Err(e) => {
                eprintln!("Warning: failed to parse rewatch.toml: {e}");
                None
            }
        }
    }
}

