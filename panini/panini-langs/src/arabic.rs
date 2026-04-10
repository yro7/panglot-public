use serde::{Deserialize, Serialize};

use panini_core::traits::{BinaryGender, BinaryVoice, LinguisticDefinition, Person, Script, TernaryNumber};

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, panini_macro::MorphologyInfo)]
#[serde(rename_all = "snake_case")]
pub enum ArabicMorphology {
    Adjective {
        lemma: String,
        root: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
        definiteness: ArabicDefiniteness,
    },
    Adposition { lemma: String },
    Adverb { lemma: String },
    Auxiliary { lemma: String },
    CoordinatingConjunction { lemma: String },
    Determiner { lemma: String },
    Interjection { lemma: String },
    Noun {
        lemma: String,
        root: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
        state: ArabicState,
        definiteness: ArabicDefiniteness,
    },
    Numeral {
        lemma: String,
        gender: BinaryGender,
        number: TernaryNumber,
        case: ArabicCase,
    },
    Particle {
        lemma: String,
        function: ArabicParticleFunction,
    },
    Pronoun {
        lemma: String,
        pronoun_type: ArabicPronounType,
        attachment_type: ArabicAttachmentType,
        person: Person,
        number: TernaryNumber,
        gender: BinaryGender,
    },
    ProperNoun { lemma: String },
    Punctuation { lemma: String },
    SubordinatingConjunction { lemma: String },
    Symbol { lemma: String },
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
    Other { lemma: String },
}

pub struct Arabic;

impl LinguisticDefinition for Arabic {
    type Morphology = ArabicMorphology;
    type GrammaticalFunction = ();

    const ISO_CODE: &'static str = "ara";

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
}
