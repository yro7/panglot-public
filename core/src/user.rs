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
    #[serde(default = "default_srs")]
    pub srs_algorithm: String,
}

fn default_srs() -> String {
    "sm2".to_string()
}

impl UserSettings {
    pub fn new(ui_language: String) -> Self {
        Self {
            ui_language,
            linguistic_background: Vec::new(),
            srs_algorithm: default_srs(),
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new("English".to_string())
    }
}
