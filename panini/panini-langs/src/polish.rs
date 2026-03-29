use serde::{Deserialize, Serialize};

use panini_core::traits::{LinguisticDefinition, IsoLang, Script, SlavicAspect, TypologicalFeature};

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
pub enum PolishTense {
    Past,
    Present,
    Future,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, panini_macro::MorphologyInfo)]
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
        aspect: SlavicAspect,
    },
    /// Other (X) for unanalyzable tokens.
    Other { lemma: String },
}

pub struct Polish;

impl LinguisticDefinition for Polish {
    type Morphology = PolishMorphology;
    type GrammaticalFunction = ();

    fn iso_code(&self) -> IsoLang {
        IsoLang::Pol
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
    }
}
