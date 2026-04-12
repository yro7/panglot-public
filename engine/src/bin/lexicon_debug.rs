use lc_core::aggregable::digest::Aggregator;
use lc_core::db::LocalStorageProvider;
use lc_core::domain::CardMetadata;
use lc_core::storage::{StorageProvider, StoredCard};
use panini_core::Aggregable;
use panini_core::aggregable::digest::{AggregationResult, BasicAggregator};

use langs::tur::TurkishGrammaticalFunction;
use langs::{ArabicMorphology, TurkishMorphology};

const DB_PATH: &str = "output/panglot.db";
const USER_ID: &str = "80512005-26ae-4cce-9cdd-48ccc1a3d950";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Lexicon Digest (Turkish) ===");

    let mut morph_agg = BasicAggregator::new();
    let mut seg_agg = BasicAggregator::new();

    let mut ara_morph_agg = BasicAggregator::new();
    let mut ara_root_agg = BasicAggregator::new();

    if let Ok(init) = LocalStorageProvider::init(DB_PATH).await {
        let provider = LocalStorageProvider::for_user(init.pool, USER_ID.to_string());
        if let Ok(cards) = provider.fetch_cards().await {
            for card in &cards {
                if let Some(metadata) = extract_metadata::<TurkishMorphology, TurkishGrammaticalFunction>(card) {
                    if metadata.language != "tur" {
                        continue;
                    }

                    // Morphology aggregation
                    let features = metadata
                        .target_features
                        .iter()
                        .chain(metadata.context_features.iter());
                    for feature in features {
                        morph_agg.record(feature);
                    }

                    // Morpheme segmentation aggregation (Turkish-specific)
                    if let Some(segs) = &metadata.morpheme_segmentation {
                        for seg in segs {
                            seg_agg.record(seg);
                        }
                    }

                    for mwe in &metadata.multiword_expressions {
                        println!("[MWE] {}", mwe.text);
                    }
                }

                // Arabic processing
                if let Some(metadata) = extract_metadata::<ArabicMorphology, ()>(card) {
                    if metadata.language != "ara" {
                        continue;
                    }

                    let features = metadata
                        .target_features
                        .iter()
                        .chain(metadata.context_features.iter());
                    for feature in features {
                        ara_morph_agg.record(feature);
                        // Pivot: bucket by root instead of PoS
                        ara_root_agg.record(&feature.pivoted(|f| {
                            f.morphology.root().unwrap_or_else(|| f.group_key())
                        }));
                    }
                }
            }
        }
    }

    let morph_result: AggregationResult = morph_agg.finish();
    let seg_result: AggregationResult = seg_agg.finish();

    if morph_result.by_group.is_empty() {
        println!("(No data in DB — injecting mock data)");
        // For mock data, recreate aggregators
        let mut morph_mock = BasicAggregator::new();
        let mut seg_mock = BasicAggregator::new();
        inject_mock_data(&mut morph_mock, &mut seg_mock);
        morph_mock.finish().print();
        seg_mock.finish().print();
    } else {
        println!("\n--- Morphology ---");
        morph_result.print();

        if !seg_result.by_group.is_empty() {
            println!("\n--- Morpheme Segmentation ---");
            seg_result.print();
        }
    }

    // Arabic Results
    let ara_morph_result = ara_morph_agg.finish();
    if true { // Forced for demonstration
        println!("\n=== Lexicon Digest (Arabic) ===");
        println!("(No data in DB — injecting mock data)");
        let mut mock_agg = BasicAggregator::new();
        let mut mock_root = BasicAggregator::new();
        inject_arabic_mock_data(&mut mock_agg, &mut mock_root);
        
        println!("\n--- Standard Aggregation (by PoS) ---");
        mock_agg.finish().print();
        
        println!("\n--- Specialized Aggregation (by Root) ---");
        mock_root.finish().print();
    } else {
        println!("\n=== Lexicon Digest (Arabic) ===");
        println!("\n--- Standard Aggregation (by PoS) ---");
        ara_morph_result.print();
        
        println!("\n--- Specialized Aggregation (by Root) ---");
        ara_root_agg.finish().print();
    }

    Ok(())
}

fn extract_metadata<M, F>(card: &StoredCard) -> Option<CardMetadata<M, F>>
where
    M: for<'de> serde::Deserialize<'de>,
    F: for<'de> serde::Deserialize<'de>,
{
    let fields: Vec<&str> = card.fields.split('\x1f').collect();
    for field in fields.into_iter().rev() {
        if field.trim().starts_with('{') {
            if let Ok(metadata) = serde_json::from_str::<CardMetadata<M, F>>(field) {
                return Some(metadata);
            }
        }
    }
    None
}

fn inject_mock_data(morph_agg: &mut BasicAggregator, seg_agg: &mut BasicAggregator) {
    use langs::tur::{TurkishCase, TurkishGrammaticalFunction, TurkishTense};
    use lc_core::morpheme::{ExtractedMorpheme, WordSegmentation};
    use lc_core::traits::BinaryNumber;

    // Mock morphology features
    let features = vec![
        langs::TurkishMorphology::Noun {
            lemma: "okul".to_string(),
            case: TurkishCase::Dative,
            number: BinaryNumber::Singular,
        },
        langs::TurkishMorphology::Noun {
            lemma: "elma".to_string(),
            case: TurkishCase::Accusative,
            number: BinaryNumber::Singular,
        },
        langs::TurkishMorphology::Noun {
            lemma: "okul".to_string(),
            case: TurkishCase::Nominative,
            number: BinaryNumber::Plural,
        },
    ];
    for feature in features {
        morph_agg.record(&feature);
    }

    // Mock morpheme segmentation
    let segs = vec![
        WordSegmentation {
            word: "okula".to_string(),
            morphemes: vec![ExtractedMorpheme {
                surface: "a".to_string(),
                base_form: "DA".to_string(),
                function: TurkishGrammaticalFunction::Case {
                    value: TurkishCase::Dative,
                },
            }],
        },
        WordSegmentation {
            word: "gidiyorum".to_string(),
            morphemes: vec![
                ExtractedMorpheme {
                    surface: "iyor".to_string(),
                    base_form: "(I)yor".to_string(),
                    function: TurkishGrammaticalFunction::Tense {
                        value: TurkishTense::Present,
                    },
                },
                ExtractedMorpheme {
                    surface: "um".to_string(),
                    base_form: "(y)Im".to_string(),
                    function: TurkishGrammaticalFunction::Agreement {
                        person: lc_core::traits::Person::First,
                        number: BinaryNumber::Singular,
                    },
                },
            ],
        },
    ];
    for seg in segs {
        seg_agg.record(&seg);
    }
}

fn inject_arabic_mock_data(morph_agg: &mut BasicAggregator, root_agg: &mut BasicAggregator) {
    use langs::arabic::{ArabicCase, ArabicDefiniteness, ArabicMorphology, ArabicTense, ArabicVerbForm, ArabicState, ArabicMood};
    use lc_core::traits::{BinaryGender, BinaryVoice, Person, TernaryNumber};

    let features = vec![
        ArabicMorphology::Noun {
            lemma: "kitāb".to_string(),
            root: "ktb".to_string(),
            pattern: None,
            gender: BinaryGender::Masculine,
            number: TernaryNumber::Singular,
            case: ArabicCase::Nominative,
            state: ArabicState::Absolute,
            definiteness: ArabicDefiniteness::Indefinite,
        },
        ArabicMorphology::Verb {
            lemma: "kataba".to_string(),
            root: "ktb".to_string(),
            form: ArabicVerbForm::I,
            person: Person::Third,
            number: TernaryNumber::Singular,
            gender: BinaryGender::Masculine,
            tense: ArabicTense::Past,
            mood: ArabicMood::Indicative,
            voice: BinaryVoice::Active,
        },
        ArabicMorphology::Noun {
            lemma: "kātib".to_string(),
            root: "ktb".to_string(),
            pattern: None,
            gender: BinaryGender::Masculine,
            number: TernaryNumber::Singular,
            case: ArabicCase::Nominative,
            state: ArabicState::Absolute,
            definiteness: ArabicDefiniteness::Indefinite,
        },
        ArabicMorphology::Noun {
            lemma: "maktaba".to_string(),
            root: "ktb".to_string(),
            pattern: None,
            gender: BinaryGender::Feminine,
            number: TernaryNumber::Singular,
            case: ArabicCase::Nominative,
            state: ArabicState::Absolute,
            definiteness: ArabicDefiniteness::Indefinite,
        },
        ArabicMorphology::Noun {
            lemma: "mudarris".to_string(),
            root: "drs".to_string(),
            pattern: None,
            gender: BinaryGender::Masculine,
            number: TernaryNumber::Singular,
            case: ArabicCase::Nominative,
            state: ArabicState::Absolute,
            definiteness: ArabicDefiniteness::Indefinite,
        },
        ArabicMorphology::Noun {
            lemma: "madrasa".to_string(),
            root: "drs".to_string(),
            pattern: None,
            gender: BinaryGender::Feminine,
            number: TernaryNumber::Singular,
            case: ArabicCase::Nominative,
            state: ArabicState::Absolute,
            definiteness: ArabicDefiniteness::Indefinite,
        },
        ArabicMorphology::Verb {
            lemma: "darasa".to_string(),
            root: "drs".to_string(),
            form: ArabicVerbForm::I,
            person: Person::Third,
            number: TernaryNumber::Singular,
            gender: BinaryGender::Masculine,
            tense: ArabicTense::Past,
            mood: ArabicMood::Indicative,
            voice: BinaryVoice::Active,
        },
    ];

    for feature in features {
        morph_agg.record(&feature);
        root_agg.record(&feature.pivoted(|m: &ArabicMorphology| m.root().unwrap_or_else(|| m.group_key())));
    }
}
