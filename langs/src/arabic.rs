use serde::{Deserialize, Serialize};

use lc_core::traits::{BinaryGender, BinaryVoice, IpaConfig, Language, Person, Script, TernaryNumber, TtsConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicCase {
    Nominative,
    Accusative,
    Genitive,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicTense {
    Past,
    Present,
    Future,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicMood {
    Indicative,
    Subjunctive,
    Jussive,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicState {
    Construct,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicDefiniteness {
    Definite,
    Indefinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicPronounType {
    Independent,
    Attached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicParticleFunction {
    Negation,
    Interrogative,
    Vocative,
    Future,
    Conjunction,
    Emphatic,
    Exception,
    Attention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArabicAttachmentType {
    Possessive,
    Object,
    Prepositional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ArabicVerbForm {
    I, II, III, IV, V, VI, VII, VIII, IX, X, XI, XII, XIII, XIV, XV,
}

/// A morphological feature representing a Part of Speech (PoS) in Modern Standard Arabic.
/// The `pos` field determines the variant type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]
#[serde(rename_all = "snake_case")]
pub enum ArabicMorphology {
    /// An adjective (ADJ).
    Adjective {
        lemma: String,
        root: String,
        /// The morphological pattern (wazn). Only provided if the adjective is derived.
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
        definiteness: ArabicDefiniteness,
    },
    /// An adposition (ADP) - replaces Preposition.
    Adposition { lemma: String },
    /// An adverb (ADV).
    Adverb { lemma: String },
    /// An auxiliary (AUX).
    Auxiliary { lemma: String },
    /// A coordinating conjunction (CCONJ).
    CoordinatingConjunction { lemma: String },
    /// A determiner (DET).
    Determiner { lemma: String },
    /// An interjection (INTJ).
    Interjection { lemma: String },
    /// A noun (NOUN).
    Noun {
        lemma: String,
        root: String,
        /// The morphological pattern (wazn). Only provided if the noun is derived from a root.
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
        state: ArabicState,
        definiteness: ArabicDefiniteness,
    },
    /// A numeral (NUM).
    Numeral {
        lemma: String,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
    },
    /// A particle (PART).
    Particle {
        lemma: String,
        function: ArabicParticleFunction,
    },
    /// A pronoun (PRON).
    Pronoun {
        lemma: String,
        pronoun_type: ArabicPronounType,
        attachment_type: ArabicAttachmentType,
        person: Person,
        number: TernaryNumber,
        gender: BinaryGender,
    },
    /// A proper noun (PROPN).
    ProperNoun { lemma: String },
    /// Punctuation (PUNCT).
    Punctuation { lemma: String },
    /// A subordinating conjunction (SCONJ).
    SubordinatingConjunction { lemma: String },
    /// A symbol (SYM).
    Symbol { lemma: String },
    /// A verb (VERB).
    Verb {
        lemma: String,
        root: String,
        form: ArabicVerbForm,
        person: Person,
        number: TernaryNumber,
        gender: BinaryGender,
        tense: ArabicTense,
        mood: ArabicMood,
        voice: BinaryVoice,
    },
    /// Other (X) for unanalyzable tokens.
    Other { lemma: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArabicExtraFields {
    /// The fully vowelled text for disambiguation and phonetic processing.
    pub context_disambiguation: String,
}

pub struct Arabic;

impl Language for Arabic {
    type Morphology = ArabicMorphology;
    type ExtraFields = ArabicExtraFields;

    fn iso_code(&self) -> lc_core::traits::IsoLang {
        lc_core::traits::IsoLang::Ara
    }

    fn supported_scripts(&self) -> &[Script] {
        &[Script::ARAB]
    }

    fn default_script(&self) -> Script {
        Script::ARAB
    }

    fn extraction_directives(&self) -> &str {
        "1. Extract lemma and root.\n\
         2. Provide the pattern only if the word is derived.\n\
         3. For nouns: include gender, number, case, state, and definiteness.\n\
         4. For adjectives: include gender, number, case, and definiteness.\n\
         5. For verbs: include form (I-X), person, number, gender, tense, mood, and voice.\n\
         6. For pronouns: specify independent or attached, person, number, and gender.\n\
         7. Separate clitics from the base word."
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("Output Modern Standard Arabic without diacritics unless explicitly required. Ensure correct agreement for duals and reverse gender agreement for numbers 3-10.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("ara-Arab")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "ar-SA-HamedNeural" }
    }

}