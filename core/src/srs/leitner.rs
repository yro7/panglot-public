use super::{Rating, ReviewEvent, SchedulingOutput, SrsAlgorithm};

const MS_PER_DAY: f64 = 86_400_000.0;
const BOX_INTERVALS: [f64; 5] = [1.0, 3.0, 7.0, 14.0, 30.0];
const MAX_BOX: usize = 4; // 0-indexed, so boxes 0..=4

pub struct Leitner;

impl Leitner {
    /// Replay the history to determine the current box, then apply the new rating.
    fn compute(history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        // Replay history to determine current box
        let mut current_box: usize = 0;

        for event in history {
            current_box = next_box(current_box, event.rating);
        }

        // Apply the new rating
        current_box = next_box(current_box, rating);
        let interval = BOX_INTERVALS[current_box];
        let due_date = now + (interval * MS_PER_DAY) as i64;

        SchedulingOutput {
            due_date,
            interval_days: interval,
        }
    }
}

fn next_box(current: usize, rating: Rating) -> usize {
    match rating {
        Rating::Again => 0,
        Rating::Hard => {
            if current == 0 { 0 } else { current - 1 }
        }
        Rating::Good | Rating::Easy => {
            if current >= MAX_BOX { MAX_BOX } else { current + 1 }
        }
    }
}

impl SrsAlgorithm for Leitner {
    fn id(&self) -> &'static str { "leitner" }
    fn name(&self) -> &'static str { "Leitner Box System" }

    fn schedule(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        Self::compute(history, rating, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;
    const DAY: i64 = 86_400_000;

    fn make_history(ratings: &[Rating]) -> Vec<ReviewEvent> {
        ratings.iter().enumerate().map(|(i, &r)| ReviewEvent {
            rating: r,
            reviewed_at: NOW + (i as i64) * DAY,
        }).collect()
    }

    // ── Happy paths ──

    #[test]
    fn progression_good() {
        let algo = Leitner;
        // New card + Good → box 1 (interval 3d)
        let out = algo.schedule(&[], Rating::Good, NOW);
        assert_eq!(out.interval_days, 3.0);

        // After one Good → box 2 (interval 7d)
        let h = make_history(&[Rating::Good]);
        let out = algo.schedule(&h, Rating::Good, NOW + DAY);
        assert_eq!(out.interval_days, 7.0);

        // After two Goods → box 3 (interval 14d)
        let h = make_history(&[Rating::Good, Rating::Good]);
        let out = algo.schedule(&h, Rating::Good, NOW + 2 * DAY);
        assert_eq!(out.interval_days, 14.0);
    }

    #[test]
    fn again_returns_to_box_0() {
        let algo = Leitner;
        // Progress to box 3, then Again
        let h = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        let out = algo.schedule(&h, Rating::Again, NOW + 5 * DAY);
        assert_eq!(out.interval_days, 1.0); // box 0
    }

    #[test]
    fn hard_descends_one_box() {
        let algo = Leitner;
        // Progress to box 3 (after 3 Goods → box 3)
        let h = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        let out = algo.schedule(&h, Rating::Hard, NOW + 5 * DAY);
        assert_eq!(out.interval_days, 7.0); // box 2
    }

    // ── Edge cases ──

    #[test]
    fn new_card_again_stays_box_0() {
        let algo = Leitner;
        let out = algo.schedule(&[], Rating::Again, NOW);
        assert_eq!(out.interval_days, 1.0);
    }

    #[test]
    fn hard_at_box_0_stays_box_0() {
        let algo = Leitner;
        let out = algo.schedule(&[], Rating::Hard, NOW);
        assert_eq!(out.interval_days, 1.0); // can't go below box 0
    }

    #[test]
    fn max_box_cap() {
        let algo = Leitner;
        // Progress to max box (4) with 5 Goods (start at 0, each Good +1)
        // 0 → 1 → 2 → 3 → 4
        let h = make_history(&[Rating::Good, Rating::Good, Rating::Good, Rating::Good]);
        // Now at box 4, another Good should stay at box 4
        let out = algo.schedule(&h, Rating::Good, NOW + 10 * DAY);
        assert_eq!(out.interval_days, 30.0); // box 4 max

        // And one more Good should still be box 4
        let h2 = make_history(&[Rating::Good, Rating::Good, Rating::Good, Rating::Good, Rating::Good]);
        let out2 = algo.schedule(&h2, Rating::Good, NOW + 20 * DAY);
        assert_eq!(out2.interval_days, 30.0);
    }

    #[test]
    fn long_history_with_alternations() {
        let algo = Leitner;
        // 30+ reviews with various ratings
        let mut ratings = Vec::new();
        for i in 0..30 {
            if i % 5 == 0 { ratings.push(Rating::Again); }
            else if i % 7 == 0 { ratings.push(Rating::Hard); }
            else { ratings.push(Rating::Good); }
        }
        let h = make_history(&ratings);
        let out = algo.schedule(&h, Rating::Good, NOW + 50 * DAY);
        // Should produce a valid box (interval in known set)
        assert!(BOX_INTERVALS.contains(&out.interval_days),
            "interval {} should be one of the box intervals", out.interval_days);
    }

    #[test]
    fn all_again_stays_box_0() {
        let algo = Leitner;
        let h = make_history(&vec![Rating::Again; 10]);
        let out = algo.schedule(&h, Rating::Again, NOW + 20 * DAY);
        assert_eq!(out.interval_days, 1.0);
    }
}
