use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default = "Config::default")]
pub struct Config {
    pub output_dir: String,
    pub hotkey: String,
    pub recording_mode: String,
    pub framerate: u32,
    pub show_overlay: bool,
    pub first_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: dirs::video_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .to_string(),
            hotkey: "Ctrl+Shift+R".to_string(),
            recording_mode: "fullscreen".to_string(),
            framerate: 30,
            show_overlay: true,
            first_run: true,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("dr-record");
        fs::create_dir_all(&path).ok();
        path.join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        tracing::info!("Loading config from: {:?}", path);

        if !path.exists() {
            tracing::info!("No config found, using defaults");
            let config = Config::default();
            config.save();
            return config;
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => {
                    tracing::info!("Config loaded successfully");
                    config
                }
                Err(e) => {
                    tracing::error!("Failed to parse config: {}. Using defaults.", e);
                    let config = Config::default();
                    config.save();
                    config
                }
            },
            Err(e) => {
                tracing::error!("Failed to read config: {}. Using defaults.", e);
                Config::default()
            }
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        tracing::info!("Saving config to: {:?}", path);

        match serde_json::to_string_pretty(self) {
            Ok(json) => match fs::write(&path, &json) {
                Ok(_) => tracing::info!("Config saved"),
                Err(e) => tracing::error!("Failed to write config: {}", e),
            },
            Err(e) => tracing::error!("Failed to serialize config: {}", e),
        }
    }
}
