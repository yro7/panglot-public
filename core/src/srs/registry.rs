use std::collections::HashMap;
use super::traits::SrsAlgorithm;
use super::sm2;
use super::leitner;
use super::fsrs;

// ── Registry ──

pub struct SrsRegistry {
    algorithms: HashMap<&'static str, Box<dyn SrsAlgorithm>>,
}

impl Default for SrsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SrsRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            algorithms: HashMap::new(),
        };
        registry.register(Box::new(sm2::Sm2));
        registry.register(Box::new(leitner::Leitner));
        registry.register(Box::new(fsrs::Fsrs::new()));
        registry.register(Box::new(fsrs::Fsrs5::new()));
        registry.register(Box::new(fsrs::Fsrs6::new()));
        registry
    }

    fn register(&mut self, algo: Box<dyn SrsAlgorithm>) {
        self.algorithms.insert(algo.id(), algo);
    }

    pub fn get(&self, id: &str) -> Option<&dyn SrsAlgorithm> {
        self.algorithms.get(id).map(AsRef::as_ref)
    }

    /// # Panics
    ///
    /// Panics if the "sm2" algorithm is not registered.
    pub fn default(&self) -> &dyn SrsAlgorithm {
        self.get("sm2").expect("SM-2 must always be registered")
    }

    pub fn list(&self) -> Vec<(&'static str, &'static str)> {
        let mut out: Vec<_> = self.algorithms.values()
            .map(|a| (a.id(), a.name()))
            .collect();
        out.sort_by_key(|&(id, _)| id);
        out
    }
}
