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
pub struct UserProfile {
    pub ui_language: String,
    pub linguistic_background: Vec<KnownLanguage>,
}

impl UserProfile {
    pub fn new(ui_language: String) -> Self {
        Self {
            ui_language,
            linguistic_background: Vec::new(),
        }
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self::new("English".to_string())
    }
}
