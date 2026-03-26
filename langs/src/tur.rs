use serde::{Deserialize, Serialize};

use lc_core::traits::{BinaryNumber, IpaConfig, Language, NoExtraFields, Person, Script, TtsConfig, TypologicalFeature};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishCase {
    /// Yalın hâl
    Nominative,
    /// Belirtme hâli
    Accusative,
    /// Yönelme hâli
    Dative,
    /// Bulunma hâli
    Locative,
    /// Ayrılma hâli
    Ablative,
    /// Tamlayan hâli
    Genitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishTense {
    /// Geçmiş zaman
    Past,
    /// Şimdiki zaman
    Present,
    /// Gelecek zaman
    Future,
    /// Geniş zaman
    Aorist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishMood {
    /// Bildirme kipi
    Indicative,
    /// Emir kipi
    Imperative,
    /// Gereklilik kipi
    Necessitative,
    /// İstek kipi
    Optative,
    /// Şart kipi
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishVoice {
    /// Etken çatı
    Active,
    /// Edilgen çatı
    Passive,
    /// Dönüşlü çatı
    Reflexive,
    /// İşteş çatı
    Reciprocal,
    /// Ettirgen çatı
    Causative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishPolarity {
    /// Olumlu
    Positive,
    /// Olumsuz
    Negative,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]
#[serde(tag = "pos")]
#[serde(rename_all = "lowercase")]
pub enum TurkishMorphology {
    /// An adjective (ADJ). (Sıfat)
    Adjective { lemma: String },
    /// An adposition (ADP) - Turkish uses postpositions. (Edat)
    Adposition { lemma: String },
    /// An adverb (ADV). (Zarf)
    Adverb { lemma: String },
    /// An auxiliary (AUX). (Yardımcı fiil)
    Auxiliary { lemma: String },
    /// A coordinating conjunction (CCONJ). (Bağlaç)
    CoordinatingConjunction { lemma: String },
    /// A determiner (DET). (Belirteç)
    Determiner { lemma: String },
    /// An interjection (INTJ). (Ünlem)
    Interjection { lemma: String },
    /// A noun (NOUN). (İsim)
    Noun {
        lemma: String,
        case: TurkishCase,
        number: BinaryNumber,
    },
    /// A numeral (NUM). (Sayı)
    Numeral { lemma: String },
    /// A particle (PART) - e.g., question particle 'mi'. (Edat/Soru eki)
    Particle { lemma: String },
    /// A pronoun (PRON). (Zamir)
    Pronoun {
        lemma: String,
        case: TurkishCase,
        number: BinaryNumber,
        person: Person,
    },
    /// A proper noun (PROPN). (Özel isim)
    ProperNoun {
        lemma: String,
        case: TurkishCase,
        number: BinaryNumber,
    },
    /// Punctuation (PUNCT). (Noktalama işareti)
    Punctuation { lemma: String },
    /// A subordinating conjunction (SCONJ). (Yan bağlaç)
    SubordinatingConjunction { lemma: String },
    /// A symbol (SYM). (Sembol)
    Symbol { lemma: String },
    /// A verb (VERB). (Fiil)
    Verb {
        lemma: String,
        tense: TurkishTense,
        mood: TurkishMood,
        voice: TurkishVoice,
        person: Person,
        number: BinaryNumber,
        polarity: TurkishPolarity,
    },
    /// Other (X) for unanalyzable tokens. (Diğer)
    Other { lemma: String },
}

pub struct Turkish;

impl Language for Turkish {
    type Morphology = TurkishMorphology;
    type ExtraFields = NoExtraFields;

    fn iso_code(&self) -> lc_core::traits::IsoLang {
        lc_core::traits::IsoLang::Tur
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
        "1. Lemmatization: All extracted words must be in their dictionary form (e.g., nouns in nominative singular, verbs in infinitive form).\n\
         2. Agglutination: Turkish is highly agglutinative. Extract the base lemma and then identify and separate all attached suffixes, providing their grammatical function.\n\
         3. Vowel Harmony: Be mindful of vowel harmony rules when analyzing suffixes, but the lemma should be the base form.\n\
         4. For nouns and proper nouns: provide the grammatical case (nominative, accusative, dative, locative, ablative, genitive) and number (singular, plural) as used in the sentence.\n\
         5. For verbs: provide the tense (past, present, future, aorist), mood (indicative, imperative, necessitative, optative, conditional), voice (active, passive, reflexive, reciprocal, causative), person (1st, 2nd, 3rd), number (singular, plural), and polarity (positive, negative).\n\
         6. For pronouns: provide the grammatical case, number, and person.\n\
         7. Question Particle 'mi': Extract the question particle 'mi' (and its vowel-harmonized variants) as a separate particle."
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Turkish text, strictly adhere to vowel harmony rules for all suffixes. Ensure correct agreement in case, number, person, tense, mood, and voice. Turkish is a pro-drop language; omit subject pronouns when contextually clear.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("tur-Latn")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "tr-TR-AhmetNeural" }
    }
}