use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: String,
    /// Polling interval for `git fetch`, in milliseconds (default 30000 = 30s).
    pub fetch_interval_ms: u32,
    pub ai_provider: String,
    pub ai_model: Option<String>,
    pub ai_api_key: Option<String>,
    pub auto_fetch: bool,
    pub syntax_highlighting: bool,
    // Added fields
    #[serde(default = "default_scan_paths")]
    pub scan_paths: Vec<String>,
    #[serde(default = "default_scan_depth")]
    pub scan_depth: u32,
    #[serde(default)]
    pub side_by_side_diff: bool,
    #[serde(default)]
    pub hide_whitespace: bool,
    /// Wrap long diff lines to fit the viewer width. Off → horizontal scroll
    /// (the original behaviour, virtualized for huge diffs). Defaults to on
    /// for new users so a wide line stays in view.
    #[serde(default = "default_wrap_long_lines")]
    pub wrap_long_lines: bool,
    #[serde(default = "default_tab_size")]
    pub tab_size: u32,
    #[serde(default = "default_claude_timeout")]
    pub claude_timeout_secs: u32,
    #[serde(default = "default_ollama_url")]
    pub ollama_server_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReposState {
    pub last_opened_repo: Option<String>,
    /// Parent folder the user last cloned into. The Clone dialog pre-fills this
    /// so repeated clones land in the same place; falls back to the first
    /// `scan_path` (then `~/Dev`) the first time.
    #[serde(default)]
    pub last_clone_dir: Option<String>,
}

fn default_scan_paths() -> Vec<String> {
    vec![
        "~/Dev".to_string(),
        "~/dev".to_string(),
        "~/code".to_string(),
        "~/Code".to_string(),
        "~/Projects".to_string(),
        "~/src".to_string(),
    ]
}

fn default_scan_depth() -> u32 {
    3
}

fn default_tab_size() -> u32 {
    4
}

fn default_wrap_long_lines() -> bool {
    true
}

fn default_claude_timeout() -> u32 {
    120
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            fetch_interval_ms: 30000,
            ai_provider: "claude".to_string(),
            ai_model: None,
            ai_api_key: None,
            auto_fetch: true,
            syntax_highlighting: true,
            scan_paths: default_scan_paths(),
            scan_depth: default_scan_depth(),
            side_by_side_diff: false,
            hide_whitespace: false,
            wrap_long_lines: default_wrap_long_lines(),
            tab_size: default_tab_size(),
            claude_timeout_secs: default_claude_timeout(),
            ollama_server_url: default_ollama_url(),
        }
    }
}

fn config_dir() -> Result<PathBuf, String> {
    if let Some(dirs) = directories::BaseDirs::new() {
        let config_path = dirs.config_dir().join("leogit");
        fs::create_dir_all(&config_path)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
        Ok(config_path)
    } else {
        Err("Could not determine config directory".to_string())
    }
}

fn config_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.toml"))
}

fn state_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("repos-state.json"))
}

#[tauri::command]
pub fn load_config() -> Result<Config, String> {
    let path = config_file()?;
    if !path.exists() {
        // First run: write defaults to disk so users can discover all options
        let cfg = Config::default();
        let content = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        if let Err(e) = fs::write(&path, content) {
            // Non-fatal: still return defaults to caller
            eprintln!("Warning: could not write default config file: {}", e);
        }
        return Ok(cfg);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {}", e))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
}

#[tauri::command]
pub fn save_config(cfg: Config) -> Result<(), String> {
    let path = config_file()?;
    let content =
        toml::to_string_pretty(&cfg).map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write config file: {}", e))
}

#[tauri::command]
pub fn load_state() -> Result<ReposState, String> {
    let path = state_file()?;
    if !path.exists() {
        return Ok(ReposState::default());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read state file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse state: {}", e))
}

#[tauri::command]
pub fn save_state(state: ReposState) -> Result<(), String> {
    let path = state_file()?;
    let content =
        serde_json::to_string_pretty(&state).map_err(|e| format!("Failed to serialize state: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write state file: {}", e))
}
