use serde::{Deserialize, Serialize};

use lc_core::morpheme::{Agglutinative, MorphemeDefinition, WordSegmentation};
use lc_core::traits::{BinaryNumber, IpaConfig, Language, MorphologyInfo, NoExtraFields, Person, Script, TtsConfig, TypologicalFeature};

// ─── Existing Turkish grammatical enums ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishCase {
    Nominative,  // Yalın hâl
    Accusative,  // Belirtme hâli
    Dative,      // Yönelme hâli
    Locative,    // Bulunma hâli
    Ablative,    // Ayrılma hâli
    Genitive,    // Tamlayan hâli
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishTense {
    Past,     // Geçmiş zaman
    Present,  // Şimdiki zaman
    Future,   // Gelecek zaman
    Aorist,   // Geniş zaman
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishMood {
    Indicative,     // Bildirme kipi
    Imperative,     // Emir kipi
    Necessitative,  // Gereklilik kipi
    Optative,       // İstek kipi
    Conditional,    // Şart kipi
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishVoice {
    Active,      // Etken çatı
    Passive,     // Edilgen çatı
    Reflexive,   // Dönüşlü çatı
    Reciprocal,  // İşteş çatı
    Causative,   // Ettirgen çatı
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishPolarity {
    Positive,  // Olumlu
    Negative,  // Olumsuz
}

// ─── New enums for morpheme-level functions ───────────────────────────────────

// Derivational functions (suffixes that change word category or meaning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishDerivation {
    Nominalization,        // -mA: verbal noun
    ActionNominalization,  // -(y)Iş: act-of-doing nominalization
    FactNominalization,    // -DIk: factive participle
    AgentSuffix,           // -CI: agent suffix
    AbstractSuffix,        // -lIk: abstract/quality suffix
    Privative,             // -sIz: "without"
    Possessional,          // -lI: "with / having"
}

// Copula / epistemic functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurkishCopula {
    Epistemic,  // -DIr: certainty / inference copula
}

// ─── GrammaticalFunction wrapper enum ────────────────────────────────────────

// All grammatical functions a Turkish morpheme can carry.
// Tagged by "category", e.g. {"category": "case", "value": "accusative"}.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum TurkishGrammaticalFunction {
    Case { value: TurkishCase },
    Tense { value: TurkishTense },
    Mood { value: TurkishMood },
    Voice { value: TurkishVoice },
    Polarity { value: TurkishPolarity },
    Number { value: BinaryNumber },
    Agreement { person: Person, number: BinaryNumber },
    Possessive { person: Person, number: BinaryNumber },
    Derivation { value: TurkishDerivation },
    Copula { value: TurkishCopula },
}

impl TurkishGrammaticalFunction {
    /// Compact representation matching serde values, for use in LLM directives.
    fn directive_label(&self) -> String {
        // Serialize to JSON, extract the relevant fields as a compact string.
        // This guarantees the labels always match the actual serde output.
        let json = serde_json::to_value(self).unwrap();
        let cat = json["category"].as_str().unwrap();
        match self {
            Self::Agreement { .. } | Self::Possessive { .. } => {
                let p = json["person"].as_str().unwrap();
                let n = json["number"].as_str().unwrap();
                format!("{cat}:{p} {n}")
            }
            _ => {
                let val = json["value"].as_str().unwrap();
                format!("{cat}:{val}")
            }
        }
    }
}

// ─── TurkishMorphology ────────────────────────────────────────────────────────

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

// ─── Static morpheme inventory ────────────────────────────────────────────────

/// Convenience alias for the PosTag type generated by the macro.
type P = TurkishMorphologyPosTag;
type F = TurkishGrammaticalFunction;

static TURKISH_MORPHEMES: &[MorphemeDefinition<F, P>] = &[
    // === Cases (nominal) ===
    MorphemeDefinition {
        base_form: "(y)I",
        functions: &[TurkishGrammaticalFunction::Case { value: TurkishCase::Accusative }],
        applies_to: &[P::Noun, P::Pronoun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "DA",
        functions: &[TurkishGrammaticalFunction::Case { value: TurkishCase::Locative }],
        applies_to: &[P::Noun, P::Pronoun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "DAn",
        functions: &[TurkishGrammaticalFunction::Case { value: TurkishCase::Ablative }],
        applies_to: &[P::Noun, P::Pronoun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(y)A",
        functions: &[TurkishGrammaticalFunction::Case { value: TurkishCase::Dative }],
        applies_to: &[P::Noun, P::Pronoun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(n)In",
        functions: &[TurkishGrammaticalFunction::Case { value: TurkishCase::Genitive }],
        applies_to: &[P::Noun, P::Pronoun, P::ProperNoun],
    },
    // === Plural ===
    MorphemeDefinition {
        base_form: "lAr",
        functions: &[
            TurkishGrammaticalFunction::Number { value: BinaryNumber::Plural },
            TurkishGrammaticalFunction::Agreement { person: Person::Third, number: BinaryNumber::Plural },
        ],
        applies_to: &[P::Noun, P::Pronoun, P::Verb, P::ProperNoun],
    },
    // === Possessive ===
    MorphemeDefinition {
        base_form: "(I)m",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::First, number: BinaryNumber::Singular }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(I)n",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::Second, number: BinaryNumber::Singular }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(s)I",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::Third, number: BinaryNumber::Singular }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(I)mIz",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::First, number: BinaryNumber::Plural }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "(I)nIz",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::Second, number: BinaryNumber::Plural }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    MorphemeDefinition {
        base_form: "lArI",
        functions: &[TurkishGrammaticalFunction::Possessive { person: Person::Third, number: BinaryNumber::Plural }],
        applies_to: &[P::Noun, P::ProperNoun],
    },
    // === Polarity (negation) ===
    MorphemeDefinition {
        base_form: "mA",
        functions: &[
            TurkishGrammaticalFunction::Polarity { value: TurkishPolarity::Negative },
            TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::Nominalization },
        ],
        applies_to: &[P::Verb],
    },
    // === Voice ===
    MorphemeDefinition {
        base_form: "(I)l",
        functions: &[TurkishGrammaticalFunction::Voice { value: TurkishVoice::Passive }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(I)n",
        functions: &[TurkishGrammaticalFunction::Voice { value: TurkishVoice::Reflexive }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(I)ş",
        functions: &[
            TurkishGrammaticalFunction::Voice { value: TurkishVoice::Reciprocal },
            TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::ActionNominalization },
        ],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "DIr",
        functions: &[
            TurkishGrammaticalFunction::Voice { value: TurkishVoice::Causative },
            TurkishGrammaticalFunction::Copula { value: TurkishCopula::Epistemic },
        ],
        applies_to: &[P::Verb, P::Noun, P::Adjective],
    },
    // === Tense / Aspect ===
    MorphemeDefinition {
        base_form: "DI",
        functions: &[TurkishGrammaticalFunction::Tense { value: TurkishTense::Past }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "mIş",
        functions: &[TurkishGrammaticalFunction::Tense { value: TurkishTense::Past }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(I)yor",
        functions: &[TurkishGrammaticalFunction::Tense { value: TurkishTense::Present }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(y)AcAk",
        functions: &[TurkishGrammaticalFunction::Tense { value: TurkishTense::Future }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(A/I)r",
        functions: &[TurkishGrammaticalFunction::Tense { value: TurkishTense::Aorist }],
        applies_to: &[P::Verb],
    },
    // === Mood ===
    MorphemeDefinition {
        base_form: "(y)sA",
        functions: &[TurkishGrammaticalFunction::Mood { value: TurkishMood::Conditional }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "mAlI",
        functions: &[TurkishGrammaticalFunction::Mood { value: TurkishMood::Necessitative }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(y)A",
        functions: &[TurkishGrammaticalFunction::Mood { value: TurkishMood::Optative }],
        applies_to: &[P::Verb],
    },
    // === Agreement (person-number on verbs) ===
    MorphemeDefinition {
        base_form: "(y)Im",
        functions: &[TurkishGrammaticalFunction::Agreement { person: Person::First, number: BinaryNumber::Singular }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "sIn",
        functions: &[TurkishGrammaticalFunction::Agreement { person: Person::Second, number: BinaryNumber::Singular }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "(y)Iz",
        functions: &[TurkishGrammaticalFunction::Agreement { person: Person::First, number: BinaryNumber::Plural }],
        applies_to: &[P::Verb],
    },
    MorphemeDefinition {
        base_form: "sInIz",
        functions: &[TurkishGrammaticalFunction::Agreement { person: Person::Second, number: BinaryNumber::Plural }],
        applies_to: &[P::Verb],
    },
    // === Derivation ===
    MorphemeDefinition {
        base_form: "CI",
        functions: &[TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::AgentSuffix }],
        applies_to: &[P::Noun, P::Verb],
    },
    MorphemeDefinition {
        base_form: "lIk",
        functions: &[TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::AbstractSuffix }],
        applies_to: &[P::Noun, P::Adjective, P::Verb],
    },
    MorphemeDefinition {
        base_form: "sIz",
        functions: &[TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::Privative }],
        applies_to: &[P::Noun],
    },
    MorphemeDefinition {
        base_form: "lI",
        functions: &[TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::Possessional }],
        applies_to: &[P::Noun],
    },
    MorphemeDefinition {
        base_form: "DIk",
        functions: &[TurkishGrammaticalFunction::Derivation { value: TurkishDerivation::FactNominalization }],
        applies_to: &[P::Verb],
    },
    // === Ability ===
    MorphemeDefinition {
        base_form: "(y)Abil",
        functions: &[TurkishGrammaticalFunction::Mood { value: TurkishMood::Optative }],
        applies_to: &[P::Verb],
    },
];

// ─── Agglutinative implementation ────────────────────────────────────────────

impl Agglutinative for Turkish {
    fn morpheme_inventory() -> &'static [MorphemeDefinition<
        TurkishGrammaticalFunction,
        <TurkishMorphology as MorphologyInfo>::PosTag,
    >] {
        TURKISH_MORPHEMES
    }

    fn morpheme_directives(&self) -> String {
        // Build inventory lines from the static TURKISH_MORPHEMES — single source of truth.
        let inventory_lines: String = TURKISH_MORPHEMES
            .iter()
            .map(|m| {
                let funcs: Vec<String> = m.functions.iter().map(|f| f.directive_label()).collect();
                format!("  {} → {}", m.base_form, funcs.join(" / "))
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "MORPHEME SEGMENTATION — fill `morpheme_segmentation` as an array of objects, \
             one per word that carries derivational or inflectional suffixes.\n\
             Each object has:\n\
             - `word`: the surface form of the word\n\
             - `morphemes`: one entry per suffix (NOT the root/stem — the root is already in `lemma`):\n\
               - `surface`: the actual allomorph as it appears (e.g. \"de\", \"yor\", \"lar\")\n\
               - `base_form`: the archiphonemic identifier from the inventory below\n\
               - `function`: {{\"category\": \"<type>\", ...value fields...}}\n\
             \n\
             <morpheme_inventory>\n\
             Use ONLY base_forms from this list:\n\
             {inventory_lines}\n\
             </morpheme_inventory>\n\
             \n\
             VOWEL HARMONY: Turkish suffixes harmonize with the preceding vowel. \
             Map surface allomorphs to the correct base_form.\n\
             ORDERING: list morphemes in the order they appear in the word (left to right).\n\
             ROOTS: do NOT include the root/stem — it is already captured in `lemma`.\n\
             Only segment words that have at least one suffix worth annotating."
        )
    }

    fn validate_and_enrich(
        &self,
        segmentation: &mut Option<Vec<WordSegmentation<TurkishGrammaticalFunction>>>,
    ) -> Result<(), String> {
        let Some(segs) = segmentation.as_mut() else {
            return Ok(());
        };

        for seg in segs.iter_mut() {
            for morpheme in seg.morphemes.iter_mut() {
                // Check base_form exists in inventory
                let definition = TURKISH_MORPHEMES
                    .iter()
                    .find(|d| d.base_form == morpheme.base_form);

                let Some(def) = definition else {
                    return Err(format!(
                        "Unknown morpheme base_form '{}' for word '{}'. Use only base_forms from the inventory.",
                        morpheme.base_form, seg.word
                    ));
                };

                // Check function is valid for this morpheme
                if !def.functions.contains(&morpheme.function) {
                    // For single-function morphemes, auto-fill if the LLM sent a wrong or empty value
                    if def.functions.len() == 1 {
                        morpheme.function = def.functions[0].clone();
                    } else {
                        return Err(format!(
                            "Invalid function {:?} for morpheme '{}' in word '{}'. Valid functions: {:?}",
                            morpheme.function, morpheme.base_form, seg.word,
                            def.functions.iter().collect::<Vec<_>>()
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── Language implementation ──────────────────────────────────────────────────

pub struct Turkish;

impl Language for Turkish {
    type Morphology = TurkishMorphology;
    type ExtraFields = NoExtraFields;
    type GrammaticalFunction = TurkishGrammaticalFunction;

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
        &[TypologicalFeature::Conjugation, TypologicalFeature::Agglutination]
    }

    fn extraction_directives(&self) -> &str {
        "1. Lemmatization: All extracted words must be in their dictionary form (e.g., nouns in nominative singular, verbs in infinitive form).\n\
         2. For nouns and proper nouns: provide the grammatical case (nominative, accusative, dative, locative, ablative, genitive) and number (singular, plural) as used in the sentence.\n\
         3. For verbs: provide the tense, mood, voice, person, number, and polarity.\n\
         4. For pronouns: provide the grammatical case, number, and person.\n\
         5. Question Particle 'mi': Extract the question particle 'mi' (and its vowel-harmonized variants) as a separate particle."
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

    // ── Agglutinative decorator overrides ────────────────────────────────────

    fn build_extraction_schema(&self) -> serde_json::Value {
        <Self as Agglutinative>::build_full_schema()
    }

    fn extra_extraction_directives(&self) -> Option<String> {
        Some(self.morpheme_directives())
    }

    fn post_process_extraction(
        &self,
        segmentation: &mut Option<Vec<WordSegmentation<TurkishGrammaticalFunction>>>,
    ) -> Result<(), String> {
        self.validate_and_enrich(segmentation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::traits::Language;

    #[test]
    fn schema_uses_grammatical_function_not_null() {
        let schema = Turkish.build_extraction_schema();
        let pretty = serde_json::to_string_pretty(&schema).unwrap();
        assert!(
            !pretty.contains("\"type\": \"null\""),
            "Schema has 'type: null' — GrammaticalFunction is resolving to () instead of TurkishGrammaticalFunction.\nSchema excerpt:\n{}",
            &pretty[..3000.min(pretty.len())]
        );
    }

    #[test]
    fn morpheme_directives_generated_from_inventory() {
        let directives = Turkish.extra_extraction_directives().unwrap();
        // Every morpheme from the inventory must appear in the directives
        for m in TURKISH_MORPHEMES {
            assert!(
                directives.contains(m.base_form),
                "Morpheme '{}' from inventory is missing in generated directives",
                m.base_form
            );
        }
        println!("{directives}");
    }
}