use serde::{Deserialize, Serialize};

use lc_core::traits::{IpaConfig, Language, NoExtraFields, Script, TtsConfig, TypologicalFeature};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanHonorifics {
    Plain,
    Polite,
    Honorific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanSpeechLevel {
    FormalDeferential,
    InformalPolite,
    Intimate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanTense {
    Past,
    Present,
    Future,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanMood {
    Indicative,
    Imperative,
    Propositive,
    Interrogative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanVoice {
    Active,
    Passive,
    Causative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanNumeralType {
    CardinalNative,
    CardinalSino,
    Ordinal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanPerson {
    First,
    Second,
    Third,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KoreanParticleFunction {
    SubjectMarker,
    ObjectMarker,
    TopicMarker,
    Possessive,
    Instrumental,
    Locative,
    Directional,
    Comitative,
    Conjunctive,
    Adverbial,
    VocativeMarker,
}

/// A morphological feature representing a Part of Speech (PoS) in Korean.
/// The `pos` field determines the variant type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]
#[serde(tag = "pos")]
#[serde(rename_all = "lowercase")]
pub enum KoreanMorphology {
    /// An adjective (형용사). In Korean, adjectives conjugate like verbs.
    Adjective {
        lemma: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        honorifics: Option<KoreanHonorifics>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tense: Option<KoreanTense>,
        #[serde(skip_serializing_if = "Option::is_none")]
        speech_level: Option<KoreanSpeechLevel>,
    },
    /// An adverb (부사).
    Adverb { lemma: String },
    /// An auxiliary (보조 용언).
    Auxiliary { lemma: String },
    /// A coordinating conjunction (접속 부사).
    CoordinatingConjunction { lemma: String },
    /// A determiner (관형사).
    Determiner { lemma: String },
    /// An interjection (감탄사).
    Interjection { lemma: String },
    /// A noun (명사).
    Noun { lemma: String },
    /// A numeral (수사).
    Numeral {
        lemma: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        numeral_type: Option<KoreanNumeralType>,
    },
    /// A grammatical particle (조사). These are postpositions that mark case, topic, etc.
    Particle {
        lemma: String,
        function: KoreanParticleFunction,
    },
    /// A pronoun (대명사).
    Pronoun {
        lemma: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        person: Option<KoreanPerson>,
        #[serde(skip_serializing_if = "Option::is_none")]
        honorifics: Option<KoreanHonorifics>,
    },
    /// A proper noun (고유 명사).
    ProperNoun { lemma: String },
    /// Punctuation (구두점).
    Punctuation { lemma: String },
    /// A subordinating conjunction (연결 어미 / 접속 부사).
    SubordinatingConjunction { lemma: String },
    /// A symbol (기호).
    Symbol { lemma: String },
    /// A verb (동사).
    Verb {
        lemma: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        honorifics: Option<KoreanHonorifics>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tense: Option<KoreanTense>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mood: Option<KoreanMood>,
        #[serde(skip_serializing_if = "Option::is_none")]
        speech_level: Option<KoreanSpeechLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        voice: Option<KoreanVoice>,
    },
    /// Other (기타) for unanalyzable tokens.
    Other { lemma: String },
}

pub struct Korean;

impl Language for Korean {
    type Morphology = KoreanMorphology;
    type ExtraFields = NoExtraFields;

    fn iso_code(&self) -> lc_core::traits::IsoLang {
        lc_core::traits::IsoLang::Kor
    }

    fn supported_scripts(&self) -> &[Script] {
        &[Script::HANI] // Hangul is primary, Hanja is secondary/historical
    }

    fn default_script(&self) -> Script {
        Script::HANG
    }

    fn typological_features(&self) -> &[TypologicalFeature] {
        &[TypologicalFeature::Conjugation] // Korean is highly agglutinative with extensive conjugation.
    }

    fn extraction_directives(&self) -> &str {
        "1. De-agglutination: Aggressively split agglutinated verbal and adjectival phrases \
         into their constituent morphemes. Isolate the main root from its suffixes and auxiliaries \
         (e.g., tense, mood, honorifics, speech level, passive, causative).\n\
         2. Dictionary Form (원형): All extracted roots (verbs, adjectives) must be converted \
         to their standard dictionary form (ending in -다).\n\
         3. Particles (조사): Isolate particles (postpositions) and extract them with their \
         grammatical function (e.g., '은/는' as 'topic_marker', '이/가' as 'subject_marker', \
         '을/를' as 'object_marker').\n\
         4. Honorifics and Speech Levels: Extract honorific markers and speech level endings \
         as distinct features for verbs, adjectives, and pronouns.\n\
         5. For verbs (동사) and adjectives (형용사): provide lemma, honorifics, tense, mood, \
         speech level, and voice (for verbs).\n\
         6. For pronouns (대명사): provide lemma, person, and honorifics.\n\
         7. For numerals (수사): provide lemma and numeral type (e.g., 'cardinal_native', 'cardinal_sino')."
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Korean text, ensure correct usage of honorifics and speech levels based on context and speaker/listener relationship. Korean is a pro-drop language; omit subject pronouns when contextually clear. Adjectives conjugate like verbs.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("kor-Hang") // Epitran supports Korean Hangul to IPA
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "ko-KR-SunHiNeural" } // A suitable Korean voice from Edge TTS
    }
}