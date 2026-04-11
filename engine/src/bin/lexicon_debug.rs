use engine::digest::LexiconDigest;
use langs::tur::TurkishGrammaticalFunction;
use langs::TurkishMorphology;
use lc_core::db::LocalStorageProvider;
use lc_core::domain::CardMetadata;
use lc_core::storage::{StorageProvider, StoredCard};

const DB_PATH: &str = "output/panglot.db";
const USER_ID: &str = "80512005-26ae-4cce-9cdd-48ccc1a3d950";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Lexicon Digest (Turkish) ===");

    let mut morph_digest = LexiconDigest::default();
    let mut seg_digest = LexiconDigest::default();

    if let Ok(init) = LocalStorageProvider::init(DB_PATH).await {
        let provider = LocalStorageProvider::for_user(init.pool, USER_ID.to_string());
        if let Ok(cards) = provider.fetch_cards().await {
            for card in &cards {
                if let Some(metadata) = extract_metadata::<TurkishMorphology, TurkishGrammaticalFunction>(card) {
                    if metadata.language != "tur" {
                        continue;
                    }

                    // Morphology digest
                    let features = metadata
                        .target_features
                        .iter()
                        .chain(metadata.context_features.iter());
                    let card_morph = LexiconDigest::from_iter(features.cloned());
                    morph_digest.merge(card_morph);

                    // Morpheme segmentation digest (Turkish-specific)
                    if let Some(segs) = metadata.morpheme_segmentation {
                        let card_seg = LexiconDigest::from_iter(segs);
                        seg_digest.merge(card_seg);
                    }

                    for mwe in &metadata.multiword_expressions {
                        println!("[MWE] {}", mwe.text);
                    }
                }
            }
        }
    }

    if morph_digest.by_pos.is_empty() {
        println!("(No data in DB — injecting mock data)");
        inject_mock_data(&mut morph_digest, &mut seg_digest);
    }

    println!("\n--- Morphology ---");
    morph_digest.print();

    if !seg_digest.by_pos.is_empty() {
        println!("\n--- Morpheme Segmentation ---");
        seg_digest.print();
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

fn inject_mock_data(morph_digest: &mut LexiconDigest, seg_digest: &mut LexiconDigest) {
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
    *morph_digest = LexiconDigest::from_iter(features);

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
    *seg_digest = LexiconDigest::from_iter(segs);
}
