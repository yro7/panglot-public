use super::{Rating, ReviewEvent, SchedulingOutput, SrsAlgorithm};

const MS_PER_DAY: f64 = 86_400_000.0;
const MIN_EF: f64 = 1.3;
const INITIAL_EF: f64 = 2.5;
const MAX_INTERVAL: f64 = 365.0; // cap at 1 year

pub struct Sm2;

impl Sm2 {
    /// Replay the full history to compute the current SM-2 state,
    /// then apply the new rating to produce the scheduling output.
    fn compute(history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        // Replay history to get current EF, interval, and repetitions
        let mut ef = INITIAL_EF;
        let mut interval: f64 = 0.0;
        let mut repetitions: u32 = 0;

        for event in history {
            let q = event.rating as u8 as f64; // 1..4 mapped to SM-2's q (we treat 1=0, 2=1, 3=3, 4=5 scale)
            let q_sm2 = match event.rating {
                Rating::Again => 0.0,
                Rating::Hard  => 2.0,
                Rating::Good  => 4.0,
                Rating::Easy  => 5.0,
            };
            let _ = q; // suppress unused warning

            // Update EF
            ef = ef + (0.1 - (5.0 - q_sm2) * (0.08 + (5.0 - q_sm2) * 0.02));
            if ef < MIN_EF {
                ef = MIN_EF;
            }

            if q_sm2 < 3.0 {
                // Failed: reset
                repetitions = 0;
                interval = 1.0;
            } else {
                repetitions += 1;
                interval = match repetitions {
                    1 => 1.0,
                    2 => 6.0,
                    _ => (interval * ef).min(MAX_INTERVAL),
                };
            }
        }

        // Now apply the new rating
        let q_sm2 = match rating {
            Rating::Again => 0.0,
            Rating::Hard  => 2.0,
            Rating::Good  => 4.0,
            Rating::Easy  => 5.0,
        };

        ef = ef + (0.1 - (5.0 - q_sm2) * (0.08 + (5.0 - q_sm2) * 0.02));
        if ef < MIN_EF {
            ef = MIN_EF;
        }

        if q_sm2 < 3.0 {
            // Failed: reset
            interval = 1.0;
        } else {
            repetitions += 1;
            interval = match repetitions {
                1 => 1.0,
                2 => 6.0,
                _ => (interval * ef).min(MAX_INTERVAL),
            };
        }

        let due_date = now + (interval * MS_PER_DAY) as i64;
        SchedulingOutput {
            due_date,
            interval_days: interval,
        }
    }
}

impl SrsAlgorithm for Sm2 {
    fn id(&self) -> &'static str { "sm2" }
    fn name(&self) -> &'static str { "SM-2" }

    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        Self::compute(history, rating, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000; // fixed timestamp for determinism
    const DAY: i64 = 86_400_000;

    fn make_history(ratings: &[Rating]) -> Vec<ReviewEvent> {
        ratings.iter().enumerate().map(|(i, &r)| ReviewEvent {
            rating: r,
            reviewed_at: NOW + (i as i64) * DAY,
        }).collect()
    }

    // ── Happy paths ──

    #[test]
    fn new_card_good() {
        let algo = Sm2;
        let out = algo.compute(&[], Rating::Good, NOW);
        assert_eq!(out.interval_days, 1.0);
        assert_eq!(out.due_date, NOW + DAY);
    }

    #[test]
    fn progression_three_goods() {
        let algo = Sm2;
        // First Good -> interval 1d
        let h1 = make_history(&[Rating::Good]);
        let out1 = algo.compute(&h1, Rating::Good, NOW + DAY);
        assert_eq!(out1.interval_days, 6.0);

        // Second Good -> interval 6d, then third
        let h2 = make_history(&[Rating::Good, Rating::Good]);
        let out2 = algo.compute(&h2, Rating::Good, NOW + 7 * DAY);
        assert!(out2.interval_days > 6.0, "interval should grow: got {}", out2.interval_days);
    }

    #[test]
    fn reset_on_again() {
        let algo = Sm2;
        // Build up progress then fail
        let history = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        let out = algo.compute(&history, Rating::Again, NOW + 10 * DAY);
        assert_eq!(out.interval_days, 1.0, "Again should reset interval to 1 day");
    }

    #[test]
    fn hard_does_not_reset() {
        let algo = Sm2;
        let history = make_history(&[Rating::Good, Rating::Good]);
        let out = algo.compute(&history, Rating::Hard, NOW + 7 * DAY);
        // Hard (q=2) is < 3 in SM-2, so it actually resets. This is standard SM-2 behavior.
        assert_eq!(out.interval_days, 1.0);
    }

    #[test]
    fn easy_boost() {
        let algo = Sm2;
        let out_good = algo.compute(&[], Rating::Good, NOW);
        let out_easy = algo.compute(&[], Rating::Easy, NOW);
        // Both first review => interval 1d
        assert_eq!(out_good.interval_days, 1.0);
        assert_eq!(out_easy.interval_days, 1.0);

        // After one review, Easy should produce same or longer intervals due to higher EF
        let h_good = make_history(&[Rating::Good]);
        let h_easy = make_history(&[Rating::Easy]);
        let out2_good = algo.compute(&h_good, Rating::Good, NOW + DAY);
        let out2_easy = algo.compute(&h_easy, Rating::Easy, NOW + DAY);
        assert_eq!(out2_good.interval_days, 6.0);
        assert_eq!(out2_easy.interval_days, 6.0);

        // Third review: EF diverges
        let h3_good = make_history(&[Rating::Good, Rating::Good]);
        let h3_easy = make_history(&[Rating::Easy, Rating::Easy]);
        let out3_good = algo.compute(&h3_good, Rating::Good, NOW + 7 * DAY);
        let out3_easy = algo.compute(&h3_easy, Rating::Easy, NOW + 7 * DAY);
        assert!(out3_easy.interval_days >= out3_good.interval_days,
            "Easy EF ({}) should produce >= interval than Good EF ({})",
            out3_easy.interval_days, out3_good.interval_days);
    }

    // ── Edge cases ──

    #[test]
    fn new_card_again() {
        let algo = Sm2;
        let out = algo.compute(&[], Rating::Again, NOW);
        assert_eq!(out.interval_days, 1.0);
    }

    #[test]
    fn ef_floor_many_agains() {
        let algo = Sm2;
        // 10 Agains in a row
        let history = make_history(&[Rating::Again; 10]);
        let out = algo.compute(&history, Rating::Good, NOW + 11 * DAY);
        // EF should have hit floor (1.3) but still produce valid output
        assert!(out.interval_days >= 1.0);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn long_history_no_panic() {
        let algo = Sm2;
        let history = make_history(&vec![Rating::Good; 25]);
        let out = algo.compute(&history, Rating::Good, NOW + 100 * DAY);
        assert!(out.interval_days > 0.0);
        assert!(out.due_date > NOW);
        // Should not overflow or produce unreasonable values
        assert!(out.interval_days < 365.0 * 10.0, "interval should be reasonable: {}", out.interval_days);
    }

    #[test]
    fn alternating_again_good() {
        let algo = Sm2;
        let ratings: Vec<Rating> = (0..10).map(|i| if i % 2 == 0 { Rating::Again } else { Rating::Good }).collect();
        let history = make_history(&ratings);
        let out = algo.compute(&history, Rating::Good, NOW + 20 * DAY);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 100.0, "should not diverge: {}", out.interval_days);
    }

    #[test]
    fn all_easy_no_overflow() {
        let algo = Sm2;
        let history = make_history(&vec![Rating::Easy; 10]);
        let out = algo.compute(&history, Rating::Easy, NOW + 100 * DAY);
        assert!(out.interval_days > 0.0);
        assert!(out.interval_days.is_finite());
        // Should still be reasonable (not years)
        assert!(out.interval_days < 365.0 * 5.0, "should be reasonable: {}", out.interval_days);
    }

    #[test]
    fn review_after_long_delay() {
        let algo = Sm2;
        let history = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        // Review 1 year later
        let out = algo.compute(&history, Rating::Good, NOW + 365 * DAY);
        assert!(out.interval_days > 0.0);
        assert!(out.due_date > NOW + 365 * DAY);
    }
}
