use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FluencyLevel {
    Beginner,
    Intermediate,
    Advanced,
    Fluent,
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownLanguage {
    pub iso_639_3: String,
    pub level: FluencyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub ui_language: String,
    pub linguistic_background: Vec<KnownLanguage>,
    #[serde(default = "UserSettings::default_srs")]
    pub srs_algorithm: String,
    #[serde(default = "UserSettings::default_learn_ahead")]
    pub learn_ahead_minutes: i32,
}

impl UserSettings {
    pub const DEFAULT_UI_LANGUAGE: &'static str = "English";
    pub const DEFAULT_SRS: &'static str = "sm2";
    pub const DEFAULT_LEARN_AHEAD: i32 = 20;

    fn default_srs() -> String { Self::DEFAULT_SRS.to_string() }
    fn default_learn_ahead() -> i32 { Self::DEFAULT_LEARN_AHEAD }

    pub fn new(ui_language: String, srs_algorithm: String, learn_ahead_minutes: i32) -> Self {
        Self {
            ui_language,
            linguistic_background: Vec::new(),
            srs_algorithm,
            learn_ahead_minutes,
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_UI_LANGUAGE.to_string(),
            Self::DEFAULT_SRS.to_string(),
            Self::DEFAULT_LEARN_AHEAD,
        )
    }
}
