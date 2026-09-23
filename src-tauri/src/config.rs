use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default = "Config::default")]
pub struct Config {
    pub output_dir: String,
    pub hotkey: String,
    /// Second global shortcut toggling annotation draw mode (Feature 2).
    /// Registered alongside `hotkey`; must differ from it (collision guard
    /// in `lib.rs` skips registration with a warning when equal).
    #[serde(default = "default_annotation_hotkey")]
    pub annotation_hotkey: String,
    /// "all" | "monitor:<device_id>" (e.g. "monitor:\\.\DISPLAY1") | "window:HWND".
    /// Legacy "monitor:<index>" values are still accepted and resolved at use time.
    pub recording_source: String,
    /// Kept for backward-compat deserialization; ignored on save.
    #[serde(default, skip_serializing)]
    pub recording_mode: Option<String>,
    pub framerate: u32,
    pub quality: String,
    pub show_overlay: bool,
    pub auto_start: bool,
    pub record_system_audio: bool,
    pub microphone_name: String,
    /// Mic master switch (default OFF). The dropdown only picks the device;
    /// recording uses the mic only when this is true AND a device is set.
    #[serde(default)]
    pub record_mic: bool,
    /// Auto-managed loopback latency compensation (ms, >= 0). Set by the
    /// background output-latency calibration, never by hand: it measures how
    /// late the default output device delivers and pre-skips exactly that
    /// much at mux time. 0 = unmeasured/off.
    #[serde(default)]
    pub system_delay_ms: u32,
    /// Output device the stored `system_delay_ms` was measured for.
    /// A different default device triggers recalibration on next launch.
    #[serde(default)]
    pub system_delay_device: String,
    #[serde(default)]
    pub mic_delay_ms: u32,
}

fn default_annotation_hotkey() -> String {
    "Ctrl+Shift+Alt+A".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: dirs::video_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .to_string(),
            hotkey: "Ctrl+Shift+Alt+R".to_string(),
            annotation_hotkey: default_annotation_hotkey(),
            recording_source: "all".to_string(),
            recording_mode: None,
            framerate: 60,
            quality: "high".to_string(),
            show_overlay: true,
            auto_start: {
                #[cfg(target_os = "windows")]
                {
                    use winreg::enums::HKEY_CURRENT_USER;
                    use winreg::RegKey;
                    let key = r"Software\Microsoft\Windows\CurrentVersion\Run";
                    if let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(key) {
                        run.get_value::<String, _>("Dr.Record").is_ok()
                    } else {
                        false
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    false
                }
            },
            record_system_audio: true,
            microphone_name: "None".to_string(),
            record_mic: false,
            system_delay_ms: 0,
            system_delay_device: String::new(),
            mic_delay_ms: 0,
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

        let mut config: Config = match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(c) => {
                    tracing::info!("Config loaded successfully");
                    c
                }
                Err(e) => {
                    tracing::error!("Failed to parse config: {}. Using defaults.", e);
                    let c = Config::default();
                    c.save();
                    return c;
                }
            },
            Err(e) => {
                tracing::error!("Failed to read config: {}. Using defaults.", e);
                return Config::default();
            }
        };

        // Backward-compat: migrate old recording_mode -> recording_source
        if config.recording_source.is_empty() || config.recording_source == "fullscreen" {
            if let Some(old_mode) = config.recording_mode.take() {
                config.recording_source = match old_mode.as_str() {
                    "multimonitor" => "all".to_string(),
                    "window" => "all".to_string(), // can't restore HWND; reset to all
                    _ => "all".to_string(),
                };
            } else {
                config.recording_source = "all".to_string();
            }
        }
        config.recording_mode = None;

        // Backward-compat: configs written before the annotation hotkey
        // existed (or with it cleared) fall back to the default.
        if config.annotation_hotkey.trim().is_empty() {
            config.annotation_hotkey = default_annotation_hotkey();
        }

        config
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
