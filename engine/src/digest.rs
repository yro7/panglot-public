use std::collections::HashMap;

use lc_core::aggregable::{Aggregable, FieldKind};

// ─── Dimension types ──────────────────────────────────────────────────────────

/// A closed-set dimension: all possible values are known upfront.
/// `possible` is populated at initialization from `FieldKind::Closed`.
/// `counts` may not contain all possible values (zero counts are omitted).
#[derive(Debug, Default)]
pub struct Distribution {
    pub possible: Vec<String>,
    pub counts: HashMap<String, usize>,
}

impl Distribution {
    fn new(possible: &[&'static str]) -> Self {
        Self {
            possible: possible.iter().map(|s| s.to_string()).collect(),
            counts: HashMap::new(),
        }
    }

    /// Number of distinct possible values actually observed.
    pub fn seen_count(&self) -> usize {
        self.counts.len()
    }

    /// Total number of possible values.
    pub fn total_count(&self) -> usize {
        self.possible.len()
    }
}

/// An open-set dimension: values are arbitrary strings (e.g. `lemma`, `base_form`).
#[derive(Debug, Default)]
pub struct Inventory {
    pub counts: HashMap<String, usize>,
}

/// A single dimension in a `PosDigest`.
#[derive(Debug)]
pub enum Dimension {
    Dist(Distribution),
    Inv(Inventory),
}

impl Dimension {
    fn record(&mut self, value: String) {
        match self {
            Dimension::Dist(d) => *d.counts.entry(value).or_insert(0) += 1,
            Dimension::Inv(i) => *i.counts.entry(value).or_insert(0) += 1,
        }
    }
}

// ─── PosDigest ────────────────────────────────────────────────────────────────

/// Aggregated data for a single POS group (e.g. "Noun", "Verb", "morpheme").
#[derive(Debug, Default)]
pub struct PosDigest {
    /// Total number of instances (not unique).
    pub total: usize,
    pub dimensions: HashMap<String, Dimension>,
}

impl PosDigest {
    fn from_descriptors(descriptors: &[lc_core::aggregable::FieldDescriptor]) -> Self {
        let mut dimensions = HashMap::new();
        for d in descriptors {
            let dim = match &d.kind {
                FieldKind::Closed(variants) => Dimension::Dist(Distribution::new(variants)),
                FieldKind::Open => Dimension::Inv(Inventory::default()),
            };
            dimensions.insert(d.name.clone(), dim);
        }
        Self { total: 0, dimensions }
    }
}

// ─── LexiconDigest ───────────────────────────────────────────────────────────

/// Aggregated lexicon statistics across all morphological features or morpheme segmentations.
///
/// Built via [`LexiconDigest::from_iter`] from any iterator of [`Aggregable`] items.
#[derive(Debug, Default)]
pub struct LexiconDigest {
    pub by_pos: HashMap<String, PosDigest>,
}

impl LexiconDigest {
    /// Build a digest from an iterator of `Aggregable` items.
    ///
    /// Each item contributes to a POS group (via `group_key()`), using its
    /// `instance_descriptors()` to set up the dimension schema on first encounter,
    /// then recording all `observations()`.
    pub fn from_iter<A: Aggregable>(items: impl IntoIterator<Item = A>) -> Self {
        let mut digest = Self::default();
        for item in items {
            let group = item.group_key();
            let descriptors = item.instance_descriptors();
            let pos_digest = digest
                .by_pos
                .entry(group)
                .or_insert_with(|| PosDigest::from_descriptors(&descriptors));
            pos_digest.total += 1;
            for observation in item.observations() {
                for (field, value) in observation {
                    if let Some(dim) = pos_digest.dimensions.get_mut(&field) {
                        dim.record(value);
                    }
                }
            }
        }
        digest
    }

    /// Merge another digest into this one (additive).
    pub fn merge(&mut self, other: LexiconDigest) {
        for (pos, other_digest) in other.by_pos {
            let entry = self.by_pos.entry(pos).or_default();
            entry.total += other_digest.total;
            for (field, other_dim) in other_digest.dimensions {
                match other_dim {
                    Dimension::Dist(od) => {
                        let possible = od.possible.clone();
                        let dim = entry.dimensions.entry(field).or_insert_with(|| {
                            Dimension::Dist(Distribution {
                                possible,
                                counts: HashMap::new(),
                            })
                        });
                        if let Dimension::Dist(d) = dim {
                            if d.possible.is_empty() {
                                d.possible = od.possible;
                            }
                            for (v, c) in od.counts {
                                *d.counts.entry(v).or_insert(0) += c;
                            }
                        }
                    }
                    Dimension::Inv(oi) => {
                        let dim = entry.dimensions.entry(field).or_insert_with(|| {
                            Dimension::Inv(Inventory::default())
                        });
                        if let Dimension::Inv(i) = dim {
                            for (v, c) in oi.counts {
                                *i.counts.entry(v).or_insert(0) += c;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn print(&self) {
        let mut pos_list: Vec<_> = self.by_pos.keys().collect();
        pos_list.sort();

        for pos in pos_list {
            let digest = &self.by_pos[pos];
            println!(
                "\n[{}] total: {}",
                pos.to_uppercase(),
                digest.total
            );

            let mut dim_list: Vec<_> = digest.dimensions.keys().collect();
            dim_list.sort();

            for dim_name in dim_list {
                let dim = &digest.dimensions[dim_name];
                match dim {
                    Dimension::Dist(d) => {
                        let seen = d.seen_count();
                        let total = d.total_count();
                        print!("  |- {} [{}/{}]: ", dim_name, seen, total);
                        let mut variants: Vec<_> = d.counts.iter().collect();
                        variants.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
                        let summary: Vec<_> =
                            variants.iter().map(|(k, c)| format!("{k}({c})")).collect();
                        println!("{}", summary.join(", "));
                    }
                    Dimension::Inv(i) => {
                        let unique = i.counts.len();
                        print!("  |- {} [{}unique]: ", dim_name, unique);
                        let mut entries: Vec<_> = i.counts.iter().collect();
                        entries.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
                        let summary: Vec<_> = entries
                            .iter()
                            .take(5)
                            .map(|(k, c)| format!("{k}({c})"))
                            .collect();
                        let suffix = if entries.len() > 5 { ", ..." } else { "" };
                        println!("{}{}", summary.join(", "), suffix);
                    }
                }
            }
        }
    }
}
