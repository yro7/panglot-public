use lc_core::domain::CardMetadata;
use lc_core::traits::{IpaConfig, Language, TtsConfig};

use async_trait::async_trait;
use anyhow::Result;
use crate::card_models::AnyCard;
use crate::python_sidecar::SharedSidecar;

/// Maximum text length sent to Python sidecar (IPA/TTS).
const MAX_SIDECAR_TEXT_LEN: usize = 5_000;

// ----- Early Post-Processing (parallel with FeatureExtractor) -----

/// Result of an EarlyPostProcessor — only the fields it is allowed to produce.
pub struct EarlyPostProcessResult {
    pub ipa: Option<String>,
    pub audio_file: Option<String>,
}

/// Phase 1: runs in parallel with FeatureExtractor.
/// Cannot access features or pedagogical_explanation.
#[async_trait]
pub trait EarlyPostProcessor<L: Language + Send + Sync>: Send + Sync
where L::Morphology: Send + Sync
{
    async fn process(
        &self,
        language: &L,
        card_id: &str,
        model: &AnyCard,
        extra_fields: &serde_json::Value,
    ) -> Result<EarlyPostProcessResult>;
}

// ----- Late Post-Processing (after FeatureExtractor) -----

/// Phase 2: runs after FeatureExtractor, has full access to metadata.
#[async_trait]
pub trait LatePostProcessor<L: Language + Send + Sync>: Send + Sync
where L::Morphology: Send + Sync
{
    async fn process(
        &self,
        language: &L,
        model: &AnyCard,
        extra_fields: &serde_json::Value,
        metadata: &mut CardMetadata<L::Morphology, L::GrammaticalFunction>,
    ) -> Result<()>;
}

// ----- Helpers -----

/// Checks if `extra_fields` contains an `context_disambiguation` key and returns its value.
fn get_disambiguation(extra_fields: &serde_json::Value) -> Option<String> {
    extra_fields
        .get("context_disambiguation")
        .or_else(|| extra_fields.get("ContextDisambiguation"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ----- TTS Generator -----

pub struct TtsGenerator {
    sidecar: SharedSidecar,
}

impl TtsGenerator {
    pub fn new(sidecar: SharedSidecar) -> Self {
        Self { sidecar }
    }
}

#[async_trait]
impl<L: Language + Send + Sync> EarlyPostProcessor<L> for TtsGenerator
where L::Morphology: Send + Sync
{
    async fn process(
        &self,
        language: &L,
        card_id: &str,
        model: &AnyCard,
        extra_fields: &serde_json::Value,
    ) -> Result<EarlyPostProcessResult> {
        let config = language.tts_strategy();
        let voice = match config {
            TtsConfig::Edge { voice } => voice,
            TtsConfig::None => {
                tracing::debug!(language = language.name(), "No TTS strategy configured, skipping");
                return Ok(EarlyPostProcessResult { ipa: None, audio_file: None });
            }
        };

        // Determine the text to speak: disambiguation > base text from card
        let text = get_disambiguation(extra_fields)
            .or_else(|| model.speakable_text());

        let text = match text {
            Some(t) if t.len() <= MAX_SIDECAR_TEXT_LEN => t,
            Some(t) => {
                tracing::warn!(len = t.len(), max = MAX_SIDECAR_TEXT_LEN, "TTS text too long, truncating");
                t[..MAX_SIDECAR_TEXT_LEN].to_string()
            }
            None => {
                tracing::debug!(card_id, "Skipping TTS (no speakable text)");
                return Ok(EarlyPostProcessResult { ipa: None, audio_file: None });
            }
        };

        tracing::info!(card_id, voice, "Generating TTS audio");

        // Write audio to a staging directory so DeckBuilder can find it
        let staging_dir = std::env::temp_dir().join("lc_audio");
        std::fs::create_dir_all(&staging_dir).ok();
        let filename = format!("{}.mp3", card_id);
        let output_path = staging_dir.join(&filename);
        let output_path_str = output_path.to_string_lossy().to_string();

        // Call edge-tts via Python sidecar
        let audio_file = match self.sidecar.lock().await.request_tts(voice, &text, &output_path_str).await {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::error!(%e, "Sidecar TTS error");
                None
            }
        };

        Ok(EarlyPostProcessResult { ipa: None, audio_file })
    }
}

// ----- IPA Generator -----

pub struct IpaGenerator {
    sidecar: SharedSidecar,
}

impl IpaGenerator {
    pub fn new(sidecar: SharedSidecar) -> Self {
        Self { sidecar }
    }
}

#[async_trait]
impl<L: Language + Send + Sync> EarlyPostProcessor<L> for IpaGenerator
where L::Morphology: Send + Sync
{
    async fn process(
        &self,
        language: &L,
        card_id: &str,
        model: &AnyCard,
        extra_fields: &serde_json::Value,
    ) -> Result<EarlyPostProcessResult> {
        let config = language.ipa_strategy();
        let epitran_code = match config {
            IpaConfig::Epitran(code) => code,
            IpaConfig::Custom(_) => {
                tracing::debug!("Custom IPA strategy not yet implemented, skipping");
                return Ok(EarlyPostProcessResult { ipa: None, audio_file: None });
            }
            IpaConfig::None => {
                tracing::debug!(language = language.name(), "No IPA strategy configured, skipping");
                return Ok(EarlyPostProcessResult { ipa: None, audio_file: None });
            }
        };

        // Determine the text to transliterate: disambiguation > base text from card
        let text = get_disambiguation(extra_fields)
            .or_else(|| model.speakable_text());

        let text = match text {
            Some(t) if t.len() <= MAX_SIDECAR_TEXT_LEN => t,
            Some(t) => {
                tracing::warn!(len = t.len(), max = MAX_SIDECAR_TEXT_LEN, "IPA text too long, truncating");
                t[..MAX_SIDECAR_TEXT_LEN].to_string()
            }
            None => {
                tracing::debug!(card_id, "Skipping IPA (no text to transliterate)");
                return Ok(EarlyPostProcessResult { ipa: None, audio_file: None });
            }
        };

        tracing::info!(card_id, epitran_code, "Generating IPA");

        // Call epitran via Python sidecar
        let ipa = match self.sidecar.lock().await.request_ipa(epitran_code, &text).await {
            Ok(ipa_str) if !ipa_str.is_empty() => Some(ipa_str),
            Ok(_) => None,
            Err(e) => {
                tracing::error!(%e, "Sidecar IPA error");
                None
            }
        };

        Ok(EarlyPostProcessResult { ipa, audio_file: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_cloze_tags() {
        use crate::card_models::strip_cloze_tags;
        assert_eq!(strip_cloze_tags("Czytam {{c1::książkę}}."), "Czytam książkę.");
        assert_eq!(strip_cloze_tags("No cloze here"), "No cloze here");
        assert_eq!(
            strip_cloze_tags("{{c1::Jem}} {{c2::jabłko}}."),
            "Jem jabłko."
        );
    }

    #[test]
    fn test_get_disambiguation() {
        let with = serde_json::json!({"context_disambiguation": "くるま"});
        assert_eq!(get_disambiguation(&with), Some("くるま".to_string()));

        let without = serde_json::json!({});
        assert_eq!(get_disambiguation(&without), None);
    }

    #[test]
    fn test_speakable_text() {
        use crate::card_models::{ClozeTest, CommonCardFront};
        let card = AnyCard::ClozeTest(ClozeTest {
            sentence: "Czytam {{c1::książkę}}.".to_string(),
            targets: vec!["książkę".to_string()],
            hint: None,
            common: CommonCardFront {
                translation: "I am reading a book.".to_string(),
                ipa: None,
                transliteration: None,
            },
        });
        assert_eq!(card.speakable_text(), Some("Czytam książkę.".to_string()));

        let oral = AnyCard::OralComprehension(crate::card_models::OralComprehension {
            audio_media: "audio.mp3".to_string(),
            transcript: "test".to_string(),
            targets: vec!["test".to_string()],
            common: CommonCardFront {
                translation: "test".to_string(),
                ipa: None,
                transliteration: None,
            },
        });
        assert_eq!(oral.speakable_text(), None);
    }
}
