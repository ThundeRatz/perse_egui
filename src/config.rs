#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub endpoint: String,
    pub show_sidebar: bool,
    pub show_right_sidebar: bool,
    pub theme_preference: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            endpoint: "127.0.0.1:9876".to_string(),
            show_sidebar: true,
            show_right_sidebar: true,
            theme_preference: "system".to_string(),
        }
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
    }
}
