use serde::{Deserialize, Serialize};

fn default_language() -> String {
    "en".into()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AppSettings {
    #[serde(default = "default_language")]
    pub language: String,
    /// Hide to the system tray instead of quitting when the window is closed.
    #[serde(default)]
    pub close_to_tray: bool,
    /// Register the app in the Windows Run key so it starts at logon.
    #[serde(default)]
    pub autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            close_to_tray: false,
            autostart: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_uses_defaults() {
        let settings: AppSettings = serde_json::from_str(r#"{"language":"zh"}"#).unwrap();
        assert_eq!(settings.language, "zh");
        assert!(!settings.close_to_tray);
        assert!(!settings.autostart);
        assert_eq!(
            serde_json::from_str::<AppSettings>("{}").unwrap(),
            AppSettings::default()
        );
    }
}
