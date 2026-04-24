use crate::srs::SrsAlgorithmId;
use crate::validated::LearnAheadMinutes;
use isolang::Language as IsoLanguage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluencyLevel {
    Beginner,
    Intermediate,
    Advanced,
    Fluent,
    Native,
} // TODO: look if not DRY with registry.rs

impl FluencyLevel {
    pub const fn as_panini_level(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Intermediate => "Intermediate",
            Self::Advanced => "Advanced",
            Self::Fluent => "Fluent",
            Self::Native => "Native",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownLanguage {
    pub iso_639_3: String,
    pub level: FluencyLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    pub app_locale: String,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            app_locale: UserSettings::DEFAULT_APP_LOCALE.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnerProfile {
    pub explanation_language_iso: String,
    #[serde(default)]
    pub known_languages: Vec<KnownLanguage>,
}

impl Default for LearnerProfile {
    fn default() -> Self {
        Self {
            explanation_language_iso: UserSettings::DEFAULT_EXPLANATION_LANGUAGE_ISO.to_string(),
            known_languages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SrsSelection {
    pub algorithm_id: SrsAlgorithmId,
}

impl Default for SrsSelection {
    fn default() -> Self {
        Self {
            algorithm_id: SrsAlgorithmId::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyPreferences {
    #[serde(default)]
    pub srs: SrsSelection,
    #[serde(default)]
    pub learn_ahead_minutes: LearnAheadMinutes,
}

impl Default for StudyPreferences {
    fn default() -> Self {
        Self {
            srs: SrsSelection::default(),
            learn_ahead_minutes: LearnAheadMinutes::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSettings {
    pub ui_preferences: UiPreferences,
    pub learner_profile: LearnerProfile,
    pub study_preferences: StudyPreferences,
}

impl UserSettings {
    pub const DEFAULT_APP_LOCALE: &'static str = "en";
    pub const DEFAULT_EXPLANATION_LANGUAGE_ISO: &'static str = "eng";
    pub const DEFAULT_LEARN_AHEAD: i32 = 20;

    pub fn new(
        app_locale: String,
        explanation_language_iso: String,
        srs_algorithm: SrsAlgorithmId,
        learn_ahead_minutes: i32,
    ) -> Self {
        Self {
            ui_preferences: UiPreferences { app_locale },
            learner_profile: LearnerProfile {
                explanation_language_iso,
                known_languages: Vec::new(),
            },
            study_preferences: StudyPreferences {
                srs: SrsSelection {
                    algorithm_id: srs_algorithm,
                },
                learn_ahead_minutes: LearnAheadMinutes::new(learn_ahead_minutes)
                    .unwrap_or_default(),
            },
        }
    }

    pub fn normalize_and_validate(mut self) -> Result<Self, String> {
        validate_app_locale(&self.ui_preferences.app_locale)?;
        self.learner_profile.explanation_language_iso =
            normalize_iso_639_3(&self.learner_profile.explanation_language_iso)?;
        self.learner_profile.known_languages = self
            .learner_profile
            .known_languages
            .into_iter()
            .map(KnownLanguage::normalize)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_APP_LOCALE.to_string(),
            Self::DEFAULT_EXPLANATION_LANGUAGE_ISO.to_string(),
            SrsAlgorithmId::default(),
            Self::DEFAULT_LEARN_AHEAD,
        )
    }
}

impl KnownLanguage {
    pub fn normalize(mut self) -> Result<Self, String> {
        self.iso_639_3 = normalize_iso_639_3(&self.iso_639_3)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyUserSettings {
    ui_language: Option<String>,
    #[serde(default)]
    linguistic_background: Vec<KnownLanguage>,
    srs_algorithm: Option<String>,
    learn_ahead_minutes: Option<i32>,
}

impl LegacyUserSettings {
    fn into_user_settings(self, defaults: &UserSettings) -> Result<UserSettings, String> {
        let explanation_language_iso = self
            .ui_language
            .as_deref()
            .map(|value| {
                best_effort_explanation_language_iso_from_legacy(
                    value,
                    &defaults.learner_profile.explanation_language_iso,
                )
            })
            .unwrap_or_else(|| defaults.learner_profile.explanation_language_iso.clone());

        let algorithm_id = match self.srs_algorithm {
            Some(value) => value.parse::<SrsAlgorithmId>()?,
            None => defaults.study_preferences.srs.algorithm_id,
        };

        let learn_ahead_minutes = match self.learn_ahead_minutes {
            Some(value) => LearnAheadMinutes::new(value)?,
            None => defaults.study_preferences.learn_ahead_minutes,
        };

        UserSettings {
            ui_preferences: UiPreferences {
                app_locale: defaults.ui_preferences.app_locale.clone(),
            },
            learner_profile: LearnerProfile {
                explanation_language_iso,
                known_languages: self.linguistic_background,
            },
            study_preferences: StudyPreferences {
                srs: SrsSelection { algorithm_id },
                learn_ahead_minutes,
            },
        }
        .normalize_and_validate()
    }
}

pub fn parse_persisted_user_settings(
    settings_json: &str,
    defaults: &UserSettings,
) -> Result<UserSettings, String> {
    match serde_json::from_str::<UserSettings>(settings_json) {
        Ok(settings) => settings.normalize_and_validate(),
        Err(canonical_error) => match serde_json::from_str::<LegacyUserSettings>(settings_json) {
            Ok(legacy) => legacy.into_user_settings(defaults),
            Err(legacy_error) => Err(format!(
                "Failed to parse settings as canonical JSON ({canonical_error}) or legacy JSON ({legacy_error})"
            )),
        },
    }
}

pub fn validate_app_locale(app_locale: &str) -> Result<(), String> {
    if app_locale.is_empty() {
        return Err("app_locale must not be empty".to_string());
    }

    for segment in app_locale.split('-') {
        if segment.is_empty()
            || segment.len() > 8
            || !segment.chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            return Err(format!("Invalid BCP-47 app_locale '{}'", app_locale));
        }
    }

    Ok(())
}

pub fn normalize_iso_639_3(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 3 {
        return Err(format!(
            "Language code '{}' must be a lowercase ISO 639-3 code",
            value
        ));
    }

    IsoLanguage::from_639_3(&normalized)
        .map(|lang| lang.to_639_3().to_string())
        .ok_or_else(|| format!("Unsupported ISO 639-3 language code '{}'", value))
}

pub fn explanation_language_name_from_iso(value: &str) -> String {
    IsoLanguage::from_639_3(value)
        .map(|lang| lang.to_name().to_string())
        .unwrap_or_else(|| value.to_string())
}

pub fn best_effort_explanation_language_iso_from_legacy(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();

    if let Ok(normalized) = normalize_iso_639_3(trimmed) {
        return normalized;
    }

    IsoLanguage::from_name(trimmed)
        .map(|lang| lang.to_639_3().to_string())
        .unwrap_or_else(|| fallback.to_string())
}
