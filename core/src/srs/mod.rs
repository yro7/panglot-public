pub mod sm2;
pub mod leitner;
pub mod fsrs;

use std::collections::HashMap;

// ── Rating ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Rating {
    Again = 1,
    Hard  = 2,
    Good  = 3,
    Easy  = 4,
}

impl Rating {
    pub fn from_u8(val: u8) -> Option<Rating> {
        match val {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }

    /// Parse from string (case-insensitive). Returns Again for unrecognized values.
    pub fn from_str_lossy(s: &str) -> Rating {
        match s.to_ascii_lowercase().as_str() {
            "again" | "1" => Rating::Again,
            "hard"  | "2" => Rating::Hard,
            "good"  | "3" => Rating::Good,
            "easy"  | "4" => Rating::Easy,
            _ => Rating::Again,
        }
    }
}

// ── Scheduling types ──

#[derive(Debug, Clone)]
pub struct ReviewEvent {
    pub rating: Rating,
    pub reviewed_at: i64,
}

/// Pure scheduling output — no display formatting.
#[derive(Debug, Clone)]
pub struct SchedulingOutput {
    pub due_date: i64,
    pub interval_days: f64,
}

pub struct SchedulingChoices {
    pub again: SchedulingOutput,
    pub hard:  SchedulingOutput,
    pub good:  SchedulingOutput,
    pub easy:  SchedulingOutput,
}

// ── SRS Algorithm trait ──

/// Purely algorithmic — no user_id, no DB access.
/// User scoping is handled by the DB layer before calling the trait.
/// Display formatting is done on the frontend side.
pub trait SrsAlgorithm: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    /// Compute the next scheduling after a given rating.
    /// `now` is passed explicitly for: testability, transactional consistency,
    /// and deterministic replay in `rebuild_scheduling_cache()`.
    fn schedule(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput;

    /// Preview all 4 choices before the user picks one.
    /// Default implementation calls `schedule()` 4 times.
    /// An algorithm can override for optimization (e.g. FSRS grouped computation).
    fn preview_choices(&self, history: &[ReviewEvent], now: i64) -> SchedulingChoices {
        SchedulingChoices {
            again: self.schedule(history, Rating::Again, now),
            hard:  self.schedule(history, Rating::Hard, now),
            good:  self.schedule(history, Rating::Good, now),
            easy:  self.schedule(history, Rating::Easy, now),
        }
    }
}

// ── Registry ──

pub struct SrsRegistry {
    algorithms: HashMap<&'static str, Box<dyn SrsAlgorithm>>,
}

impl SrsRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            algorithms: HashMap::new(),
        };
        registry.register(Box::new(sm2::Sm2));
        registry.register(Box::new(leitner::Leitner));
        registry.register(Box::new(fsrs::Fsrs::new()));
        registry
    }

    fn register(&mut self, algo: Box<dyn SrsAlgorithm>) {
        self.algorithms.insert(algo.id(), algo);
    }

    pub fn get(&self, id: &str) -> Option<&dyn SrsAlgorithm> {
        self.algorithms.get(id).map(|b| b.as_ref())
    }

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

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_from_u8_valid() {
        assert_eq!(Rating::from_u8(1), Some(Rating::Again));
        assert_eq!(Rating::from_u8(2), Some(Rating::Hard));
        assert_eq!(Rating::from_u8(3), Some(Rating::Good));
        assert_eq!(Rating::from_u8(4), Some(Rating::Easy));
    }

    #[test]
    fn rating_from_u8_invalid() {
        assert_eq!(Rating::from_u8(0), None);
        assert_eq!(Rating::from_u8(5), None);
        assert_eq!(Rating::from_u8(255), None);
    }

    #[test]
    fn rating_from_str_lossy_valid() {
        assert_eq!(Rating::from_str_lossy("Again"), Rating::Again);
        assert_eq!(Rating::from_str_lossy("HARD"), Rating::Hard);
        assert_eq!(Rating::from_str_lossy("good"), Rating::Good);
        assert_eq!(Rating::from_str_lossy("Easy"), Rating::Easy);
        assert_eq!(Rating::from_str_lossy("3"), Rating::Good);
    }

    #[test]
    fn rating_from_str_lossy_invalid_defaults_to_again() {
        assert_eq!(Rating::from_str_lossy("banana"), Rating::Again);
        assert_eq!(Rating::from_str_lossy(""), Rating::Again);
        assert_eq!(Rating::from_str_lossy("99"), Rating::Again);
    }

    #[test]
    fn registry_get_sm2() {
        let reg = SrsRegistry::new();
        let algo = reg.get("sm2").unwrap();
        assert_eq!(algo.id(), "sm2");
        assert_eq!(algo.name(), "SM-2");
    }

    #[test]
    fn registry_get_leitner() {
        let reg = SrsRegistry::new();
        let algo = reg.get("leitner").unwrap();
        assert_eq!(algo.id(), "leitner");
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let reg = SrsRegistry::new();
        assert!(reg.get("fsrs").is_none());
    }

    #[test]
    fn registry_default_is_sm2() {
        let reg = SrsRegistry::new();
        assert_eq!(reg.default().id(), "sm2");
    }

    #[test]
    fn registry_list_contains_all() {
        let reg = SrsRegistry::new();
        let list = reg.list();
        assert_eq!(list.len(), 3);
        let ids: Vec<&str> = list.iter().map(|&(id, _)| id).collect();
        assert!(ids.contains(&"sm2"));
        assert!(ids.contains(&"leitner"));
        assert!(ids.contains(&"fsrs"));
    }

    #[test]
    fn preview_choices_empty_history() {
        let reg = SrsRegistry::new();
        let algo = reg.default();
        let now = 1_700_000_000_000i64;
        let choices = algo.preview_choices(&[], now);

        // All 4 should produce valid outputs
        assert!(choices.again.due_date > now);
        assert!(choices.hard.due_date > now);
        assert!(choices.good.due_date > now);
        assert!(choices.easy.due_date > now);

        // Ordering: again <= hard <= good <= easy
        assert!(choices.again.due_date <= choices.hard.due_date);
        assert!(choices.hard.due_date <= choices.good.due_date);
        assert!(choices.good.due_date <= choices.easy.due_date);
    }
}
