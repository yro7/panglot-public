use lc_core::aggregable::digest::Aggregator;
use lc_core::db::LocalStorageProvider;
use lc_core::domain::CardMetadata;
use lc_core::storage::{StorageProvider, StoredCard};
use panini_core::Aggregable;
use panini_core::aggregable::digest::{AggregationResult, BasicAggregator};

use langs::tur::TurkishGrammaticalFunction;
use langs::TurkishMorphology;
use langs::arabic::ArabicMorphology;

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
                        
                        let root = feature.morphology.root().unwrap_or_else(|| feature.group_key());
                        
                        // Aggregator
                        ara_root_agg.record(&feature.pivoted(|_| root.clone()));
                    }
                }
            }
        }
    }

    let morph_result: AggregationResult = morph_agg.finish();
    let seg_result: AggregationResult = seg_agg.finish();

    if morph_result.by_group.is_empty() {
        println!("(No data in DB — skipping console print)");
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
    let ara_root_result = ara_root_agg.finish();

    println!("\n=== Lexicon Digest (Arabic) ===");
    if ara_morph_result.by_group.is_empty() {
        println!("(No data in DB — skipping console print)");
    } else {
        println!("\n--- Standard Aggregation (by PoS) ---");
        ara_morph_result.print();
        
        println!("\n--- Specialized Aggregation (by Root) ---");
        ara_root_result.print();
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
