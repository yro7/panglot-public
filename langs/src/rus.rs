use serde::{Deserialize, Serialize};

use lc_core::traits::{IpaConfig, Language, NoExtraFields, Script, TtsConfig, TypologicalFeature};

/// A morphological feature representing a Part of Speech (PoS) in Russian.
/// The `pos` field determines the variant type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]
#[serde(tag = "pos")]
#[serde(rename_all = "lowercase")]
pub enum RussianMorphology {
    /// An adjective (ADJ) - Прилагательное.
    Adjective {
        lemma: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
        degree: String,
    },
    /// An adposition (ADP) - Предлог.
    Adposition {
        lemma: String,
        /// The grammatical case this adposition governs.
        governed_case: String,
    },
    /// An adverb (ADV) - Наречие.
    Adverb {
        lemma: String,
        degree: String,
    },
    /// An auxiliary (AUX) - Вспомогательный глагол.
    Auxiliary { lemma: String },
    /// A coordinating conjunction (CCONJ) - Сочинительный союз.
    CoordinatingConjunction { lemma: String },
    /// A determiner (DET) - Определитель.
    Determiner {
        lemma: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
    },
    /// An interjection (INTJ) - Междометие.
    Interjection { lemma: String },
    /// A noun (NOUN) - Существительное.
    Noun {
        lemma: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
    },
    /// A numeral (NUM) - Числительное.
    Numeral {
        lemma: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
    },
    /// A particle (PART) - Частица.
    Particle { lemma: String },
    /// A pronoun (PRON) - Местоимение.
    Pronoun {
        lemma: String,
        person: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
    },
    /// A proper noun (PROPN) - Имя собственное.
    ProperNoun {
        lemma: String,
        gender: String,
        number: String,
        case: String,
        animacy: String,
    },
    /// Punctuation (PUNCT) - Пунктуация.
    Punctuation { lemma: String },
    /// A subordinating conjunction (SCONJ) - Подчинительный союз.
    SubordinatingConjunction { lemma: String },
    /// A symbol (SYM) - Символ.
    Symbol { lemma: String },
    /// A verb (VERB) - Глагол.
    Verb {
        lemma: String,
        aspect: String,
        tense: String,
        mood: String,
        person: String,
        number: String,
        gender: String,
        voice: String,
    },
    /// Other (X) for unanalyzable tokens - Прочее.
    Other { lemma: String },
}

pub struct Russian;

impl Language for Russian {
    type Morphology = RussianMorphology;
    type ExtraFields = NoExtraFields;

    fn iso_code(&self) -> lc_core::traits::IsoLang {
        lc_core::traits::IsoLang::Rus
    }

    fn supported_scripts(&self) -> &[Script] {
        &[Script::CYRL]
    }

    fn default_script(&self) -> Script {
        Script::CYRL
    }

    fn typological_features(&self) -> &[TypologicalFeature] {
        &[TypologicalFeature::Conjugation]
    }

    fn extraction_directives(&self) -> &str {
        "1. Lemmatization: All extracted words must be in their dictionary form \
         (nominative singular for nouns and adjectives, infinitive for verbs).\n\
         2. For nouns, adjectives, determiners, numerals, and pronouns: provide gender \
         (masculine, feminine, neuter), number (singular, plural), case \
         (nominative, genitive, dative, accusative, instrumental, prepositional), \
         and animacy (animate, inanimate).\n\
         3. For verbs: provide aspect (perfective, imperfective), tense (past, present, future), \
         mood (indicative, imperative, conditional), person (1st, 2nd, 3rd), \
         number (singular, plural), gender (masculine, feminine, neuter for past tense), \
         and voice (active, passive).\n\
         4. For adpositions: provide the case they govern (governed_case).\n\
         5. For adjectives and adverbs: provide the degree (positive, comparative, superlative)."
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Russian text, ensure correct case, gender, number, and animacy agreement between nouns, adjectives, determiners, and pronouns. Pay attention to verb aspect and tense usage. Russian is a pro-drop language; omit subject pronouns when contextually clear.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("rus-Cyrl")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge {
            voice: "ru-RU-DariyaNeural",
        }
    }
}