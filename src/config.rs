use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    CtrlAltWheel,
    AltWheel,
    Mouse4Wheel,
    Mouse5Wheel,
    F24ToggleAndWheel,
}

impl TriggerMode {
    pub fn label(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            TriggerMode::CtrlAltWheel => "Cmd/Ctrl + Option + Mouse Wheel",
            #[cfg(not(target_os = "macos"))]
            TriggerMode::CtrlAltWheel => "Ctrl + Alt + Mouse Wheel",

            #[cfg(target_os = "macos")]
            TriggerMode::AltWheel => "Option (Alt) + Mouse Wheel",
            #[cfg(not(target_os = "macos"))]
            TriggerMode::AltWheel => "Alt + Mouse Wheel",

            TriggerMode::Mouse4Wheel => "Mouse 4 (Back / XButton1) + Wheel",
            TriggerMode::Mouse5Wheel => "Mouse 5 (Forward / XButton2) + Wheel",
            TriggerMode::F24ToggleAndWheel => "F24 Hotkey Toggle / Hold + Wheel (Logi Options+)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub zoom_step: f32,
    pub max_zoom: f32,
    pub smoothness: f32,
    pub spotlight_enabled: bool,
    pub spotlight_opacity: f32, // 0.0 to 0.85
    pub spotlight_radius: f32,  // pixels in screen space (e.g. 120.0 to 600.0)
    pub trigger_mode: TriggerMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            zoom_step: 0.35,
            max_zoom: 5.0,
            smoothness: 0.18,
            spotlight_enabled: true,
            spotlight_opacity: 0.55,
            spotlight_radius: 220.0,
            trigger_mode: TriggerMode::CtrlAltWheel,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> Option<PathBuf> {
        // Windows: %APPDATA%/SmoothZoom/config.json
        // macOS: ~/Library/Application Support/SmoothZoom/config.json
        ProjectDirs::from("", "", "SmoothZoom").map(|dirs| dirs.config_dir().join("config.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<AppConfig>(&data) {
                    return cfg;
                }
            }
        }
        let default_cfg = Self::default();
        default_cfg.save();
        default_cfg
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = fs::write(path, json);
            }
        }
    }
}

pub struct SharedState {
    pub config: AppConfig,
    pub target_zoom: f32,
    pub current_zoom: f32,
    pub zoom_enabled: bool,
    pub f24_toggled: bool,
    pub show_settings: bool,
    pub should_quit: bool,
}

impl SharedState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            target_zoom: 1.0,
            current_zoom: 1.0,
            zoom_enabled: true,
            f24_toggled: false,
            show_settings: false,
            should_quit: false,
        }
    }
}
