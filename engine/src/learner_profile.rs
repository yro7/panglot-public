//! Learner profile tracking via aggregation of linguistic features.
//!
//! This module provides stateful aggregators for tracking a learner's exposure
//! to grammatical features, lemmas, and morphological patterns over time.

use std::collections::HashMap;

use lc_core::aggregable::Aggregable;
use panini_core::aggregable::digest::{AggregationResult, Aggregator};
use panini_core::component::ExtractionResult;

use crate::pipeline::worker::MorphSection;

/// Stateful aggregator for tracking a learner's linguistic profile.
///
/// Unlike `BasicAggregator`, this aggregator supports:
/// - Tracking by skill node (for progressive learning)
/// - Generating LLM-friendly summaries (`to_llm_summary()`)
/// - Identifying unseen features (`unseen_features()`)
///
/// NON-generic: can ingest heterogeneous `Aggregable` types via method-level `<A>`.
pub struct LearnerProfileAggregator {
    /// Main aggregation result (fed by all `record()` calls)
    result: AggregationResult,
    /// Optional per-skill tracking
    skill_stats: HashMap<String, AggregationResult>,
    current_skill: Option<String>,
}

impl LearnerProfileAggregator {
    pub fn new() -> Self {
        Self {
            result: AggregationResult::default(),
            skill_stats: HashMap::new(),
            current_skill: None,
        }
    }

    /// Enable per-skill tracking for subsequent records.
    pub fn set_current_skill(&mut self, skill_path: impl Into<String>) {
        self.current_skill = Some(skill_path.into());
    }

    /// Generate a compact YAML summary for LLM prompts.
    ///
    /// Format:
    /// ```yaml
    /// Noun:
    ///   total: 142
    ///   case:
    ///     coverage: "5/7"
    ///     distribution:
    ///       Nominative: 45
    ///       Accusative: 38
    ///   lemma:
    ///     unique: 87
    ///     top: [pies(12), kot(8), dom(7), ...]
    /// ```
    pub fn to_llm_summary(&self, max_lemmas: usize) -> String {
        let mut output = String::new();

        let mut groups: Vec<_> = self.result.by_group.keys().collect();
        groups.sort();

        for (i, group) in groups.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }

            let group_data = &self.result.by_group[*group];
            output.push_str(&format!("{}:\n", group));
            output.push_str(&format!("  total: {}\n", group_data.total));

            let mut dims: Vec<_> = group_data.dimensions.keys().collect();
            dims.sort();

            for dim_name in dims {
                let dim = &group_data.dimensions[dim_name];
                match dim {
                    panini_core::aggregable::digest::Dimension::Dist(d) => {
                        let (seen, total) = d.coverage();
                        output.push_str(&format!("  {}:\n", dim_name));
                        output.push_str(&format!("    coverage: \"{}/{}\"\n", seen, total));
                        output.push_str("    distribution:\n");

                        let mut variants: Vec<_> = d.counts.iter().collect();
                        variants.sort_by_key(|(_, c)| std::cmp::Reverse(**c));

                        for (value, count) in variants {
                            output.push_str(&format!("      {}: {}\n", value, count));
                        }
                    }
                    panini_core::aggregable::digest::Dimension::Inv(i) => {
                        let unique = i.counts.len();
                        output.push_str(&format!("  {}:\n", dim_name));
                        output.push_str(&format!("    unique: {}\n", unique));

                        let mut entries: Vec<_> = i.counts.iter().collect();
                        entries.sort_by_key(|(_, c)| std::cmp::Reverse(**c));

                        let top: Vec<_> = entries
                            .iter()
                            .take(max_lemmas)
                            .map(|(k, c)| format!("{}({})", k, c))
                            .collect();

                        output.push_str(&format!("    top: [{}]\n", top.join(", ")));
                    }
                }
            }
        }

        output
    }

    /// Return features that have been seen fewer than `threshold` times.
    ///
    /// Useful for prioritizing unseen grammatical features in exercise generation.
    pub fn unseen_features(&self, threshold: usize) -> Vec<FeatureKey> {
        let mut unseen = Vec::new();

        for (group, group_data) in &self.result.by_group {
            for (field, dim) in &group_data.dimensions {
                match dim {
                    panini_core::aggregable::digest::Dimension::Dist(d) => {
                        // For closed sets, check which values have low counts
                        for value in &d.possible {
                            if let Some(&count) = d.counts.get(value) {
                                if count < threshold {
                                    unseen.push(FeatureKey {
                                        group: group.clone(),
                                        field: field.clone(),
                                        value: value.clone(),
                                        count,
                                    });
                                }
                            } else {
                                unseen.push(FeatureKey {
                                    group: group.clone(),
                                    field: field.clone(),
                                    value: value.clone(),
                                    count: 0,
                                });
                            }
                        }
                    }
                    panini_core::aggregable::digest::Dimension::Inv(i) => {
                        // For open sets, return all values below threshold
                        for (value, &count) in &i.counts {
                            if count < threshold {
                                unseen.push(FeatureKey {
                                    group: group.clone(),
                                    field: field.clone(),
                                    value: value.clone(),
                                    count,
                                });
                            }
                        }
                    }
                }
            }
        }

        unseen
    }

    /// Get per-skill statistics (if tracking is enabled).
    pub fn stats_by_skill(&self) -> &HashMap<String, AggregationResult> {
        &self.skill_stats
    }

    /// Consume and return the main aggregation result.
    pub fn finish(self) -> AggregationResult {
        self.result
    }
}

impl Default for LearnerProfileAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Aggregator for LearnerProfileAggregator {
    type Output = AggregationResult;

    fn record<A: Aggregable>(&mut self, item: &A) {
        // Record to main result
        record_to_result(&mut self.result, item);

        // Also record to current skill if tracking is enabled
        if let Some(ref skill) = self.current_skill {
            let skill_result = self
                .skill_stats
                .entry(skill.clone())
                .or_insert_with(AggregationResult::default);
            record_to_result(skill_result, item);
        }
    }

    fn finish(self) -> AggregationResult {
        self.result
    }
}

/// Helper to record an Aggregable item to an AggregationResult.
///
/// Duplicates the logic from BasicAggregator::record() to avoid borrowing issues.
fn record_to_result<A: Aggregable>(result: &mut AggregationResult, item: &A) {
    let group = item.group_key();
    let descriptors = item.instance_descriptors();

    // Use entry API to get or create group
    use panini_core::aggregable::digest::{Dimension, Distribution, GroupResult, Inventory};

    let group_result = result.by_group.entry(group).or_insert_with(|| {
        // Initialize dimensions from descriptors
        let mut dimensions = HashMap::new();
        for d in descriptors {
            let dim = match &d.kind {
                panini_core::aggregable::FieldKind::Closed(variants) => {
                    Dimension::Dist(Distribution::new(variants))
                }
                panini_core::aggregable::FieldKind::Open => Dimension::Inv(Inventory::default()),
            };
            dimensions.insert(d.name.clone(), dim);
        }
        GroupResult {
            total: 0,
            dimensions,
        }
    });

    group_result.total += 1;

    for observation in item.observations() {
        for (field, value) in observation {
            if let Some(dim) = group_result.dimensions.get_mut(&field) {
                match dim {
                    Dimension::Dist(d) => *d.counts.entry(value).or_insert(0) += 1,
                    Dimension::Inv(i) => *i.counts.entry(value).or_insert(0) += 1,
                }
            }
        }
    }
}

/// Key identifying a specific linguistic feature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeatureKey {
    pub group: String, // e.g. "Noun", "Verb"
    pub field: String, // e.g. "case", "tense", "lemma"
    pub value: String, // e.g. "Nominative", "Present", "pies"
    pub count: usize,
}

// ─── Helper: aggregate_extraction ─────────────────────────────────────────────

/// Aggregate all aggregable items from an ExtractionResult.
///
/// This helper is generic over the `Aggregator` trait, allowing it to work
/// with both `BasicAggregator` and `LearnerProfileAggregator`.
///
/// Type parameters `M` and `F` are concrete (known via `Pipeline<L>`),
/// while `Agg` is generic over the `Aggregator` trait.
pub fn aggregate_extraction<M, F, Agg>(
    extraction: &ExtractionResult,
    agg: &mut Agg,
) -> Result<(), panini_core::component::ExtractionResultError>
where
    Agg: Aggregator,
    M: for<'de> serde::Deserialize<'de> + Aggregable,
    F: for<'de> serde::Deserialize<'de> + panini_core::aggregable::AggregableFields,
{
    use panini_core::morpheme::WordSegmentation;
    
    // Morphology: target_features
    if let Ok(morph) = extraction.get::<MorphSection<M>>("morphology") {
        for feature in &morph.target_features {
            agg.record(feature);
        }
        for feature in &morph.context_features {
            agg.record(feature);
        }
    }

    // Morpheme segmentation (optional, agglutinative languages only)
    if let Ok(segs_opt) =
        extraction.get::<Option<Vec<WordSegmentation<F>>>>("morpheme_segmentation")
    {
        if let Some(segs) = segs_opt {
            for seg in &segs {
                agg.record(seg);
            }
        }
    }

    Ok(())
}

// ─── Simple helper for LLM prompts ────────────────────────────────────────────

/// Build a compact LLM-friendly summary from existing cards in the database.
///
/// This is the **simplest way** to get an aggregated lexicon profile for LLM prompts.
///
/// # Example
/// ```rust
/// let provider = LocalStorageProvider::for_user(pool, user_id);
/// let summary = build_lexicon_summary_for_llm::<TurkishMorphology, TurkishGrammaticalFunction>(
///     &provider,
///     10, // max lemmas per group
/// ).await?;
///
/// // Use in GeneratorContext
/// let ctx = GeneratorContext {
///     lexicon_profile: Some(summary),
///     ..
/// };
/// ```
pub async fn build_lexicon_summary_for_llm<M, F>(
    provider: &impl lc_core::storage::StorageProvider,
    max_lemmas: usize,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
where
    M: for<'de> serde::Deserialize<'de> + Aggregable,
    F: for<'de> serde::Deserialize<'de> + panini_core::aggregable::AggregableFields,
{
    let mut agg = LearnerProfileAggregator::new();

    let cards = provider.fetch_cards().await?;
    for card in &cards {
        // Extract metadata from card fields (last field is JSON metadata)
        let fields: Vec<&str> = card.fields.split('\x1f').collect();
        for field in fields.into_iter().rev() {
            if field.trim().starts_with('{') {
                if let Ok(metadata) = serde_json::from_str::<lc_core::domain::CardMetadata<M, F>>(field) {
                    // Aggregate target and context features
                    for feature in &metadata.target_features {
                        agg.record(feature);
                    }
                    for feature in &metadata.context_features {
                        agg.record(feature);
                    }
                    
                    // Aggregate morpheme segmentation if present
                    if let Some(segs) = &metadata.morpheme_segmentation {
                        for seg in segs {
                            agg.record(seg);
                        }
                    }
                }
                break; // Found metadata, stop searching
            }
        }
    }

    Ok(agg.to_llm_summary(max_lemmas))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learner_profile_aggregator_basic() {
        let mut agg = LearnerProfileAggregator::new();
        // Would need mock Aggregable items here
        // For now, just verify construction works
        assert_eq!(agg.result.by_group.len(), 0);
    }

    #[test]
    fn to_llm_summary_format() {
        let agg = LearnerProfileAggregator::new();
        let summary = agg.to_llm_summary(5);
        assert!(summary.is_empty()); // Empty aggregator = empty summary
    }
}
