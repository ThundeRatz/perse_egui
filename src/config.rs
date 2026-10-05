use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::editor::{canvas::ViewVisibilityOptions, models::MissionSetCollection};

fn default_true() -> bool {
    true
}

fn default_mission_file_path() -> String {
    "mission_points.yaml".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub endpoint: String,
    pub show_sidebar: bool,
    pub show_right_sidebar: bool,
    pub theme_preference: String,
    pub zoom_speed: f32,
    pub custom_config_path: Option<String>,
    #[serde(default = "default_mission_file_path")]
    pub mission_file_path: String,
    #[serde(default)]
    pub canvas_display_options: ViewVisibilityOptions,
    #[serde(default = "default_true")]
    pub is_display_options_open: bool,
    #[serde(default = "default_true")]
    pub color_parameter_hierarchy: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            endpoint: "127.0.0.1:9876".to_string(),
            show_sidebar: true,
            show_right_sidebar: true,
            theme_preference: "system".to_string(),
            zoom_speed: 0.04,
            custom_config_path: None,
            mission_file_path: default_mission_file_path(),
            canvas_display_options: ViewVisibilityOptions::default(),
            is_display_options_open: true,
            color_parameter_hierarchy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PersistentAppState {
    pub config: AppConfig,
    pub mission_sets: MissionSetCollection,
}

impl PersistentAppState {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let state: PersistentAppState = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json_str = serde_json::to_string_pretty(self)?;
        std::fs::write(path_ref, json_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = AppConfig::default();
        assert_eq!(config.endpoint, "127.0.0.1:9876");
        assert!(config.show_sidebar);
        assert!(config.show_right_sidebar);
        assert_eq!(config.theme_preference, "system");
        assert_eq!(config.custom_config_path, None);
        assert_eq!(config.mission_file_path, "mission_points.yaml");
        assert!(config.color_parameter_hierarchy);
    }

    #[test]
    fn test_persistent_app_state_roundtrip() {
        let mut state = PersistentAppState::default();
        state.config.endpoint = "1.2.3.4:5678".to_string();
        state.config.custom_config_path = Some("test_config.json".to_string());

        state.config.is_display_options_open = false;
        state.config.canvas_display_options.show_margins = false;
        state.config.canvas_display_options.show_parameter_labels = false;
        state
            .config
            .canvas_display_options
            .visible_param_keys
            .insert("my_param".to_string());
        state.config.color_parameter_hierarchy = true;

        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("perse_test_persistent_config.json");

        state
            .save_to_file(&test_file)
            .expect("Should save persistent state");
        let loaded =
            PersistentAppState::load_from_file(&test_file).expect("Should load persistent state");

        assert_eq!(loaded.config.endpoint, "1.2.3.4:5678");
        assert_eq!(
            loaded.config.custom_config_path,
            Some("test_config.json".to_string())
        );
        assert!(!loaded.config.is_display_options_open);
        assert!(!loaded.config.canvas_display_options.show_margins);
        assert!(!loaded.config.canvas_display_options.show_parameter_labels);
        assert!(loaded
            .config
            .canvas_display_options
            .visible_param_keys
            .contains("my_param"));
        assert!(loaded.config.color_parameter_hierarchy);

        let _ = std::fs::remove_file(test_file);
    }
}
