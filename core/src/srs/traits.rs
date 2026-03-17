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

    /// The pure math of the algorithm, stripped of learning steps and start-of-day offsets.
    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput;

    /// Compute the next scheduling after a given rating.
    /// Includes Anki-like learning steps and start-of-day interval alignments.
    /// `now` is passed explicitly for: testability, transactional consistency,
    /// and deterministic replay in `rebuild_scheduling_cache()`.
    fn schedule(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        // 1. State machine — also collects the graduated-only history for compute().
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum CardState {
            New,
            Learning(u8), // step 1 (1m) or 2 (10m)
            Graduated,
        }

        let transition = |state: CardState, r: Rating| -> CardState {
            match state {
                CardState::New => match r {
                    Rating::Again | Rating::Hard => CardState::Learning(1),
                    Rating::Good => CardState::Learning(2),
                    Rating::Easy => CardState::Graduated,
                },
                CardState::Learning(step) => match r {
                    Rating::Again => CardState::Learning(1),
                    Rating::Hard => CardState::Learning(step), // repeat current step
                    Rating::Good => {
                        if step == 1 { CardState::Learning(2) } else { CardState::Graduated }
                    }
                    Rating::Easy => CardState::Graduated,
                },
                CardState::Graduated => match r {
                    Rating::Again => CardState::Learning(1),
                    _ => CardState::Graduated,
                },
            }
        };

        // Replay history, keeping only events that happened while the card was already
        // Graduated. Learning-phase reviews are excluded from algorithm input so that
        // same-day repetitions during initial learning don't inflate stability.
        let mut state = CardState::New;
        let mut grad_history: Vec<ReviewEvent> = Vec::new();
        for event in history {
            if state == CardState::Graduated {
                grad_history.push(event.clone());
            }
            state = transition(state, event.rating);
        }
        state = transition(state, rating);

        match state {
            CardState::Learning(1) => SchedulingOutput {
                due_date: now + 60_000,  // + 1 min
                interval_days: 0.0,
            },
            CardState::Learning(2) => SchedulingOutput {
                due_date: now + 600_000, // + 10 mins
                interval_days: 0.0,
            },
            CardState::Learning(_) => unreachable!("Invalid learning step"),
            CardState::Graduated | CardState::New => {
                // 2. Normal algorithm scheduling (learning phase excluded)
                let mut out = self.compute(&grad_history, rating, now);

                // 3. Start-of-day rounding
                if out.interval_days >= 1.0 {
                    let ms_per_day = 86_400_000;
                    let start_of_day = if now >= 0 {
                        now - (now % ms_per_day)
                    } else {
                        let rem = now % ms_per_day;
                        if rem == 0 { now } else { now - ms_per_day - rem }
                    };
                    out.due_date = start_of_day + (out.interval_days * ms_per_day as f64) as i64;
                }

                out
            }
        }
    }

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
