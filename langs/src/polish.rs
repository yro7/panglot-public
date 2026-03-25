use serde::{Deserialize, Serialize};

use lc_core::traits::{IpaConfig, Language, NoExtraFields, Script, TtsConfig, TypologicalFeature};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolishCase {
    Nominative,
    Genitive,
    Dative,
    Accusative,
    Instrumental,
    Locative,
    Vocative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolishGender {
    MasculinePersonal,
    MasculineAnimate,
    MasculineInanimate,
    Feminine,
    Neuter,
}

impl PolishGender {
    pub fn is_masculine(&self) -> bool {
        matches!(self, Self::MasculinePersonal | Self::MasculineAnimate | Self::MasculineInanimate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolishAspect {
    Perfective,
    Imperfective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolishTense {
    Past,
    Present,
    Future,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]
#[serde(tag = "pos")]
#[serde(rename_all = "snake_case")]
pub enum PolishMorphology {
    /// An adjective (ADJ).
    Adjective {
        lemma: String,
        gender: PolishGender,
        case: PolishCase,
    },
    /// An adposition (ADP) - replaces Preposition.
    Adposition {
        lemma: String,
        /// The grammatical case this adposition governs.
        governed_case: PolishCase,
    },
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
        gender: PolishGender,
        case: PolishCase,
    },
    /// A numeral (NUM).
    Numeral { lemma: String },
    /// A particle (PART).
    Particle { lemma: String },
    /// A pronoun (PRON).
    Pronoun {
        lemma: String,
        case: PolishCase,
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
        tense: PolishTense,
        aspect: PolishAspect,
    },
    /// Other (X) for unanalyzable tokens.
    Other { lemma: String },
}

pub struct Polish;

impl Language for Polish {
    type Morphology = PolishMorphology;
    type ExtraFields = NoExtraFields;

    fn iso_code(&self) -> lc_core::traits::IsoLang {
        lc_core::traits::IsoLang::Pol
    }

    fn supported_scripts(&self) -> &[Script] {
        &[Script::LATN]
    }

    fn default_script(&self) -> Script {
        Script::LATN
    }

    fn typological_features(&self) -> &[TypologicalFeature] {
        &[TypologicalFeature::Conjugation]
    }

    fn extraction_directives(&self) -> &str {
        "Do not forget to specify 'cases' when extracting the features."
        // "1. Lemmatization: All extracted words must be in their dictionary form \
        //  (mianownik for nouns, bezokolicznik for verbs, mianownik rodzaju męskiego \
        //  for adjectives).\n\
        //  2. Compound expressions: If a group of words forms a single semantic unit \
        //  (e.g., an idiom or phrasal expression), extract it as one entry.\n\
        //  3. For nouns and adjectives: provide the grammatical gender \
        //  (masculine, feminine, neuter) and the case as used in the sentence \
        //  (nominative, accusative, genitive, dative, instrumental, locative, vocative).\n\
        //  4. For verbs: provide the aspect (perfective, imperfective).\n\
        //  5. For prepositions: provide the case they govern (governed_case)."
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Polish text, use standard Polish orthography with correct diacritics. Polish is a pro-drop language; omit subject pronouns when they are contextually clear.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("pol-Latn")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "pl-PL-ZofiaNeural" }
    }

}
