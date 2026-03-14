use super::{
    ReviewEvent, SchedulingOutput, SchedulingChoices, Rating,
};

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
