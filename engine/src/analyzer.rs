use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use lc_core::storage::{StorageProvider, StoredCard};
use lc_core::domain::{CardMetadata, ExtractedFeature};
use lc_core::traits::MorphologyInfo;

// ----- Word Profile -----

/// Aggregated profile for a single word/feature across skills.
///
/// Tracks which skills a word has been mastered in and which
/// skills the learner is struggling with (leech cards).
#[derive(Debug, Clone)]
pub struct WordProfile<M: Debug + Clone> {
    pub lemma: String,
    pub morphology: M,
    /// Skills where this word appears in new cards (interval == 0).
    pub learning_skills: HashSet<String>,
    /// Skills where this word appears in learning cards (0 < interval < 21 days).
    pub learnt_skills: HashSet<String>,
    /// Skills where this word appears in mature cards (interval >= 21 days).
    pub mastered_skills: HashSet<String>,
    /// Skills where this word appears in leech-tagged cards.
    pub struggling_skills: HashSet<String>,
}

// ----- Lexicon Tracker -----

/// Aggregated index of word profiles built from Anki card metadata.
///
/// Used to determine what the learner knows and what they're struggling with,
/// enabling the generation of targeted decks following the i+1 principle.
/// Keyed by **lemma** (dictionary form) for proper morphological grouping.
pub struct LexiconTracker<M: Debug + Clone> {
    pub profiles: HashMap<String, WordProfile<M>>,
}

impl<M> LexiconTracker<M>
where
    M: Debug + Clone + PartialEq + MorphologyInfo,
{
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
        }
    }

    /// Records a feature as mastered for a given skill.
    pub fn mark_mastered(&mut self, entry: &ExtractedFeature<M>, skill_id: &str) {
        let lemma = entry.morphology.lemma().to_string();
        let profile = self
            .profiles
            .entry(lemma.clone())
            .or_insert_with(|| WordProfile {
                lemma,
                morphology: entry.morphology.clone(),
                learning_skills: HashSet::new(),
                learnt_skills: HashSet::new(),
                mastered_skills: HashSet::new(),
                struggling_skills: HashSet::new(),
            });
        profile.mastered_skills.insert(skill_id.to_string());
    }

    /// Records a feature as struggling (leech) for a given skill.
    pub fn mark_struggling(&mut self, entry: &ExtractedFeature<M>, skill_id: &str) {
        let lemma = entry.morphology.lemma().to_string();
        let profile = self
            .profiles
            .entry(lemma.clone())
            .or_insert_with(|| WordProfile {
                lemma,
                morphology: entry.morphology.clone(),
                learning_skills: HashSet::new(),
                learnt_skills: HashSet::new(),
                mastered_skills: HashSet::new(),
                struggling_skills: HashSet::new(),
            });
        profile.struggling_skills.insert(skill_id.to_string());
    }

    /// Records a feature as learning for a given skill.
    pub fn mark_learning(&mut self, entry: &ExtractedFeature<M>, skill_id: &str) {
        let lemma = entry.morphology.lemma().to_string();
        let profile = self
            .profiles
            .entry(lemma.clone())
            .or_insert_with(|| WordProfile {
                lemma,
                morphology: entry.morphology.clone(),
                learning_skills: HashSet::new(),
                learnt_skills: HashSet::new(),
                mastered_skills: HashSet::new(),
                struggling_skills: HashSet::new(),
            });
        profile.learning_skills.insert(skill_id.to_string());
    }

    /// Records a feature as learnt for a given skill.
    pub fn mark_learnt(&mut self, entry: &ExtractedFeature<M>, skill_id: &str) {
        let lemma = entry.morphology.lemma().to_string();
        let profile = self
            .profiles
            .entry(lemma.clone())
            .or_insert_with(|| WordProfile {
                lemma,
                morphology: entry.morphology.clone(),
                learning_skills: HashSet::new(),
                learnt_skills: HashSet::new(),
                mastered_skills: HashSet::new(),
                struggling_skills: HashSet::new(),
            });
        profile.learnt_skills.insert(skill_id.to_string());
    }

    /// Filters entries matching a predicate on their WordProfile.
    pub fn filter<F>(&self, predicate: F) -> Vec<ExtractedFeature<M>>
    where
        F: Fn(&WordProfile<M>) -> bool,
    {
        self.profiles
            .values()
            .filter(|p| predicate(p))
            .map(|p| ExtractedFeature {
                word: p.lemma.clone(),
                morphology: p.morphology.clone(),
            })
            .collect()
    }

    /// Returns all mastered words (words with at least one mastered skill).
    pub fn mastered_words(&self) -> Vec<ExtractedFeature<M>> {
        self.filter(|p| !p.mastered_skills.is_empty())
    }

    /// Returns all struggling words (words with at least one struggling skill).
    pub fn struggling_words(&self) -> Vec<ExtractedFeature<M>> {
        self.filter(|p| !p.struggling_skills.is_empty())
    }

    /// Returns the total number of tracked word profiles.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Returns all known (mastered) words matching a specific PoS label (e.g. "Noun", "Verb").
    pub fn get_known_by_pos(&self, pos: &str) -> Vec<ExtractedFeature<M>> {
        self.filter(|p| {
            !p.mastered_skills.is_empty() && p.morphology.pos_label() == pos
        })
    }

    /// Returns all tracked words matching a specific PoS label, known or not.
    pub fn get_all_by_pos(&self, pos: &str) -> Vec<ExtractedFeature<M>> {
        self.filter(|p| {
            p.morphology.pos_label() == pos
        })
    }

    /// Returns all tracked words, known or not.
    pub fn get_all_words(&self) -> Vec<ExtractedFeature<M>> {
        self.filter(|_| true)
    }

    // ── Convenience wrappers for each UD UPOS tag ──

    pub fn get_known_adjectives(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Adjective") }
    pub fn get_known_adpositions(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Adposition") }
    pub fn get_known_adverbs(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Adverb") }
    pub fn get_known_auxiliaries(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Auxiliary") }
    pub fn get_known_coordinating_conjunctions(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("CoordinatingConjunction") }
    pub fn get_known_determiners(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Determiner") }
    pub fn get_known_interjections(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Interjection") }
    pub fn get_known_nouns(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Noun") }
    pub fn get_known_numerals(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Numeral") }
    pub fn get_known_particles(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Particle") }
    pub fn get_known_pronouns(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Pronoun") }
    pub fn get_known_proper_nouns(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("ProperNoun") }
    pub fn get_known_punctuation(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Punctuation") }
    pub fn get_known_subordinating_conjunctions(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("SubordinatingConjunction") }
    pub fn get_known_symbols(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Symbol") }
    pub fn get_known_verbs(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Verb") }
    pub fn get_known_other(&self) -> Vec<ExtractedFeature<M>> { self.get_known_by_pos("Other") }

    /// Returns ALL words with their PoS, mastery status, and skill counts.
    pub fn all_words_with_status(&self) -> Vec<serde_json::Value>
    where
        M: Serialize,
    {
        let mut entries: Vec<serde_json::Value> = self.profiles.values().map(|p| {
            let status = if !p.struggling_skills.is_empty() {
                "struggling"
            } else if !p.mastered_skills.is_empty() {
                "mastered"
            } else if !p.learnt_skills.is_empty() {
                "learnt"
            } else {
                "learning"
            };
            serde_json::json!({
                "lemma": p.lemma,
                "pos": p.morphology.pos_label(),
                "morphology": p.morphology,
                "status": status,
                "learning_count": p.learning_skills.len(),
                "learnt_count": p.learnt_skills.len(),
                "mastered_count": p.mastered_skills.len(),
                "struggling_count": p.struggling_skills.len(),
            })
        }).collect();
        entries.sort_by(|a, b| {
            let pos_a = a["pos"].as_str().unwrap_or("");
            let pos_b = b["pos"].as_str().unwrap_or("");
            pos_a.cmp(pos_b).then_with(|| {
                let l_a = a["lemma"].as_str().unwrap_or("");
                let l_b = b["lemma"].as_str().unwrap_or("");
                l_a.cmp(l_b)
            })
        });
        entries
    }

    /// Returns a summary of mastered word counts grouped by PoS label.
    pub fn summary_by_pos(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for profile in self.profiles.values() {
            if !profile.mastered_skills.is_empty() {
                *counts.entry(profile.morphology.pos_label().to_string()).or_insert(0) += 1;
            }
        }
        counts
    }
}

impl<M: Debug + Clone + PartialEq + MorphologyInfo> Default for LexiconTracker<M> {
    fn default() -> Self {
        Self::new()
    }
}

// ----- Library Analyzer -----

/// Analyzes Anki cards to build a knowledge graph of the learner's progress.
///
/// Reads cards from an `AnkiRepository`, extracts the hidden `CardMetadata` JSON
/// from each card's fields, and populates a `LexiconTracker` with the results.
pub struct LibraryAnalyzer;

impl LibraryAnalyzer {
    /// Extracts a `LexiconTracker` from Anki cards.
    ///
    /// For each card:
    /// 1. Parse the hidden `CardMetadata<M>` JSON from the card's fields
    /// 2. Classify each feature as mastered or struggling based on the card's status
    /// 3. Aggregate into the tracker
    pub async fn extract_tracker_async<M>(
        &self,
        provider: &(dyn StorageProvider + Sync),
        language_filter: Option<&str>,
    ) -> Result<LexiconTracker<M>, Box<dyn std::error::Error + Send + Sync>>
    where
        M: Debug + Clone + PartialEq + MorphologyInfo + Serialize + for<'de> Deserialize<'de>,
    {
        let cards = provider.fetch_cards().await?;
        let mut tracker = LexiconTracker::new();

        for card in &cards {
            if let Some(metadata) = self.extract_metadata::<M>(card) {
                // Skip cards from other languages
                if let Some(lang) = language_filter {
                    if !metadata.language.is_empty() && metadata.language != lang {
                        continue;
                    }
                }
                for feature in metadata.target_features.iter().chain(metadata.context_features.iter()) {
                    if card.is_leech() {
                        tracker.mark_struggling(feature, &metadata.skill_id);
                    }

                    if card.is_mature() {
                        tracker.mark_mastered(feature, &metadata.skill_id);
                    } else if card.interval_days > 0.0 {
                        tracker.mark_learnt(feature, &metadata.skill_id);
                    } else {
                        tracker.mark_learning(feature, &metadata.skill_id);
                    }
                }
            }
        }

        Ok(tracker)
    }

    fn extract_metadata<M>(&self, card: &StoredCard) -> Option<CardMetadata<M>>
    where
        M: for<'de> Deserialize<'de>,
    {
        // In Anki, fields are separated by \x1f
        let fields: Vec<&str> = card.fields.split('\x1f').collect();

        // Try to parse JSON from any field (usually near the end).
        // If the user reorders fields in Anki, 'Metadata' is no longer strictly `.last()`.
        for field in fields.into_iter().rev() {
            if let Ok(metadata) = serde_json::from_str(field) {
                return Some(metadata);
            }
        }
        
        None
    }
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::storage::{StorageProvider, StoredCard, DeckInfo, NewDeckData};
    use langs::PolishMorphology;

    struct MockProvider {
        cards: Vec<StoredCard>,
    }

    #[async_trait::async_trait]
    impl StorageProvider for MockProvider {
        async fn fetch_cards(&self) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.cards.clone())
        }
        async fn fetch_decks(&self) -> Result<Vec<DeckInfo>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }
        async fn save_deck(&self, _deck: &NewDeckData) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
            Ok(0)
        }
    }

    fn make_card_with_metadata(
        card_id: i64,
        skill_id: &str,
        features: Vec<ExtractedFeature<PolishMorphology>>,
        interval_days: f64,
        tags: &str,
    ) -> StoredCard {
        let metadata = CardMetadata {
            card_id: format!("c{}", card_id),
            language: "pol".to_string(),
            skill_id: skill_id.to_string(),
            skill_name: String::new(),
            pedagogical_explanation: String::new(),
            target_features: features,
            context_features: vec![],
            multiword_expressions: vec![],
            ipa: None,
            audio_file: None,
        };
        let metadata_json = serde_json::to_string(&metadata).unwrap();
        // Fields format: front \x1f back \x1f metadata
        let fields = format!("front\x1fback\x1f{}", metadata_json);

        StoredCard {
            note_id: card_id.to_string(),
            card_id: card_id.to_string(),
            fields,
            tags: tags.to_string(),
            interval_days,
            lapses: 0,
        }
    }

    fn noun(word: &str, lemma: &str) -> ExtractedFeature<PolishMorphology> {
        ExtractedFeature {
            word: word.to_string(),
            morphology: PolishMorphology::Noun {
                lemma: lemma.to_string(),
                gender: String::new(),
                case: String::new(),
            },
        }
    }

    fn verb(word: &str, lemma: &str) -> ExtractedFeature<PolishMorphology> {
        ExtractedFeature {
            word: word.to_string(),
            morphology: PolishMorphology::Verb {
                lemma: lemma.to_string(),
                tense: String::new(),
                aspect: String::new(),
            },
        }
    }

    #[test]
    fn tracker_mark_mastered() {
        let mut tracker: LexiconTracker<PolishMorphology> = LexiconTracker::new();
        let entry = noun("dom", "dom");
        tracker.mark_mastered(&entry, "polish_nom");
        assert_eq!(tracker.len(), 1);
        assert!(tracker.profiles["dom"].mastered_skills.contains("polish_nom"));
    }

    #[test]
    fn tracker_mark_struggling() {
        let mut tracker: LexiconTracker<PolishMorphology> = LexiconTracker::new();
        let entry = verb("mówić", "mówić");
        tracker.mark_struggling(&entry, "polish_past");
        assert_eq!(tracker.struggling_words().len(), 1);
    }

    #[test]
    fn tracker_filter_mastered_words() {
        let mut tracker: LexiconTracker<PolishMorphology> = LexiconTracker::new();
        let n = noun("dom", "dom");
        let v = verb("iść", "iść");

        tracker.mark_mastered(&n, "skill_a");
        tracker.mark_struggling(&v, "skill_b");

        let mastered = tracker.mastered_words();
        assert_eq!(mastered.len(), 1);
        assert_eq!(mastered[0].word, "dom");
    }

    #[tokio::test]
    async fn analyzer_extracts_tracker_from_mock_repo() {
        let cards = vec![
            make_card_with_metadata(
                1,
                "polish_acc",
                vec![
                    noun("książkę", "książka"),
                    verb("czytać", "czytać"),
                ],
                30.0, // mature
                "grammar",
            ),
            make_card_with_metadata(
                2,
                "polish_nom",
                vec![noun("dom", "dom")],
                5.0, // not mature
                "leech vocabulary", // struggling
            ),
            make_card_with_metadata(
                3,
                "polish_gen",
                vec![noun("książki", "książka")],
                25.0, // mature
                "leech", // also struggling
            ),
        ];

        let provider = MockProvider { cards };
        let analyzer = LibraryAnalyzer;
        let tracker: LexiconTracker<PolishMorphology> = analyzer.extract_tracker_async(&provider, Some("pol")).await.unwrap();

        // książka: mastered in polish_acc and polish_gen, struggling in polish_gen
        // Now keyed by lemma "książka" (not surface form)
        let profile = &tracker.profiles["książka"];
        assert!(profile.mastered_skills.contains("polish_acc"));
        assert!(profile.mastered_skills.contains("polish_gen"));
        assert!(profile.struggling_skills.contains("polish_gen"));

        // czytać: mastered in polish_acc
        let profile = &tracker.profiles["czytać"];
        assert!(profile.mastered_skills.contains("polish_acc"));
        assert!(profile.struggling_skills.is_empty());

        // dom: struggling in polish_nom, not mature
        let profile = &tracker.profiles["dom"];
        assert!(profile.mastered_skills.is_empty());
        assert!(profile.struggling_skills.contains("polish_nom"));
        assert!(profile.learnt_skills.contains("polish_nom")); // interval is 5
    }

    #[tokio::test]
    async fn analyzer_ignores_cards_without_metadata() {
        let cards = vec![StoredCard {
            note_id: "1".to_string(),
            card_id: "1".to_string(),
            fields: "just a normal card\x1fwith no metadata".to_string(),
            tags: "".to_string(),
            interval_days: 30.0,
            lapses: 0,
        }];

        let provider = MockProvider { cards };
        let analyzer = LibraryAnalyzer;
        let tracker: LexiconTracker<PolishMorphology> = analyzer.extract_tracker_async(&provider, Some("pol")).await.unwrap();

        assert!(tracker.is_empty());
    }

    #[test]
    fn test_get_known_by_pos_and_summary() {
        let mut tracker = LexiconTracker::new();

        tracker.mark_mastered(&ExtractedFeature {
            word: "książkę".to_string(),
            morphology: PolishMorphology::Noun {
                lemma: "książka".to_string(),
                gender: "Feminine".to_string(),
                case: "Accusative".to_string(),
            },
        }, "polish_acc");

        tracker.mark_mastered(&ExtractedFeature {
            word: "czytam".to_string(),
            morphology: PolishMorphology::Verb {
                lemma: "czytać".to_string(),
                aspect: "Imperfective".to_string(),
                tense: "Present".to_string(),
            },
        }, "polish_acc");

        // Unmastered adjective
        tracker.mark_struggling(&ExtractedFeature {
            word: "nowy".to_string(),
            morphology: PolishMorphology::Adjective {
                lemma: "nowy".to_string(),
                gender: "Masculine".to_string(),
                case: "Nominative".to_string(),
            },
        }, "polish_nom");

        // get_known_by_pos
        let nouns = tracker.get_known_by_pos("Noun");
        assert_eq!(nouns.len(), 1);
        assert_eq!(nouns[0].morphology.lemma(), "książka");

        // convenience wrappers
        assert_eq!(tracker.get_known_nouns().len(), 1);
        assert_eq!(tracker.get_known_verbs().len(), 1);
        assert_eq!(tracker.get_known_adjectives().len(), 0); // struggling, not mastered

        // summary
        let summary = tracker.summary_by_pos();
        assert_eq!(summary.get("Noun"), Some(&1));
        assert_eq!(summary.get("Verb"), Some(&1));
        assert_eq!(summary.get("Adjective"), None);
    }
}
