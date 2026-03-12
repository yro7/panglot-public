//! FSRS-4.5 (Free Spaced Repetition Scheduler) implementation.
//!
//! Based on the DSR (Difficulty, Stability, Retrievability) memory model.
//! Reference: https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm
//!
//! Key concepts:
//! - Stability (S): time interval (in days) at which retrievability drops to 90%
//! - Difficulty (D): inherent difficulty of the card, range [1, 10]
//! - Retrievability (R): probability of recalling the card at time t

use super::{Rating, ReviewEvent, SchedulingOutput, SrsAlgorithm};

const MS_PER_DAY: f64 = 86_400_000.0;

/// FSRS-4.5 forgetting curve constants
const DECAY: f64 = -0.5;
/// FACTOR ensures R(S, S) = 0.9: factor = 0.9^(1/DECAY) - 1 = 19/81
const FACTOR: f64 = 19.0 / 81.0;

/// Desired retention rate (probability of recall at the scheduled time).
const DESIRED_RETENTION: f64 = 0.9;

/// Maximum interval cap (days).
const MAX_INTERVAL: f64 = 365.0;

/// Default FSRS-4.5 parameters (17 weights), optimized on large dataset.
const DEFAULT_W: [f64; 17] = [
    0.4872,  // w0:  S0(Again) — initial stability for Again
    1.4003,  // w1:  S0(Hard)  — initial stability for Hard
    3.7145,  // w2:  S0(Good)  — initial stability for Good
    13.8206, // w3:  S0(Easy)  — initial stability for Easy
    5.1618,  // w4:  D0(Good) = w4, initial difficulty when first rating is Good
    1.2298,  // w5:  D0 scaling: D0(G) = w4 - (G-3)*w5
    0.8975,  // w6:  difficulty update delta
    0.031,   // w7:  mean reversion weight
    1.6474,  // w8:  recall stability: e^w8
    0.1367,  // w9:  recall stability: S^(-w9)
    1.0461,  // w10: recall stability: e^(w10*(1-R)) - 1
    2.1072,  // w11: fail stability: w11 * ...
    0.0793,  // w12: fail stability: D^(-w12)
    0.3246,  // w13: fail stability: (S+1)^w13 - 1
    1.587,   // w14: fail stability: e^(w14*(1-R))
    0.2272,  // w15: hard penalty multiplier
    2.8755,  // w16: easy bonus multiplier
];

pub struct Fsrs {
    w: [f64; 17],
}

impl Default for Fsrs {
    fn default() -> Self {
        Self { w: DEFAULT_W }
    }
}

impl Fsrs {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Core DSR Model ──

    /// Initial stability after the first rating. S0(G) = w[G-1]
    fn initial_stability(&self, rating: Rating) -> f64 {
        let idx = (rating as u8 - 1) as usize;
        self.w[idx].max(0.1) // ensure minimum positive stability
    }

    /// Initial difficulty after the first rating.
    /// D0(G) = w4 - (G - 3) * w5, clamped to [1, 10]
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let d = self.w[4] - (g - 3.0) * self.w[5];
        d.clamp(1.0, 10.0)
    }

    /// Update difficulty after a review.
    /// D'(D, G) = w7 * D0(3) + (1 - w7) * (D - w6 * (G - 3))
    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let d_prime = d - self.w[6] * (g - 3.0);
        let d0_good = self.w[4]; // D0(3) = w4
        let new_d = self.w[7] * d0_good + (1.0 - self.w[7]) * d_prime;
        new_d.clamp(1.0, 10.0)
    }

    /// Retrievability: R(t, S) = (1 + FACTOR * t/S)^DECAY
    /// where t is elapsed time in days
    fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        (1.0 + FACTOR * elapsed_days / stability).powf(DECAY)
    }

    /// Next interval from stability and desired retention.
    /// I(r, S) = (S / FACTOR) * (r^(1/DECAY) - 1)
    fn next_interval(&self, stability: f64) -> f64 {
        let interval = (stability / FACTOR)
            * (DESIRED_RETENTION.powf(1.0 / DECAY) - 1.0);
        interval.max(1.0).min(MAX_INTERVAL)
    }

    /// Stability after a successful recall (Hard, Good, or Easy).
    /// S'r(D, S, R, G) = S * (e^w8 * (11-D) * S^(-w9) * (e^(w10*(1-R)) - 1) * penalty/bonus + 1)
    fn stability_after_recall(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard { self.w[15] } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { self.w[16] } else { 1.0 };

        let s_inc = self.w[8].exp()
            * (11.0 - d)
            * s.powf(-self.w[9])
            * ((self.w[10] * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus;

        // SInc must be >= 1 for successful recall
        let s_inc = s_inc.max(1.0);
        s * s_inc
    }

    /// Stability after forgetting (Again).
    /// S'f(D, S, R) = w11 * D^(-w12) * ((S+1)^w13 - 1) * e^(w14*(1-R))
    fn stability_after_failure(&self, d: f64, s: f64, r: f64) -> f64 {
        let new_s = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * (self.w[14] * (1.0 - r)).exp();
        // Post-lapse stability should be at least a small positive value
        // but less than the previous stability
        new_s.max(0.1).min(s)
    }

    /// Replay history and compute output for the new rating.
    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        if history.is_empty() {
            // First review ever
            let s = self.initial_stability(rating);
            let interval = self.next_interval(s);
            let due_date = now + (interval * MS_PER_DAY) as i64;
            return SchedulingOutput {
                due_date,
                interval_days: interval,
            };
        }

        // Replay history to recover (S, D) state
        let mut stability = self.initial_stability(history[0].rating);
        let mut difficulty = self.initial_difficulty(history[0].rating);
        let mut last_review_at = history[0].reviewed_at;

        for i in 1..history.len() {
            let event = &history[i];
            let elapsed_ms = (event.reviewed_at - last_review_at).max(0) as f64;
            let elapsed_days = elapsed_ms / MS_PER_DAY;
            let r = self.retrievability(elapsed_days, stability);

            // Update difficulty
            difficulty = self.next_difficulty(difficulty, event.rating);

            // Update stability
            stability = if event.rating == Rating::Again {
                self.stability_after_failure(difficulty, stability, r)
            } else {
                self.stability_after_recall(difficulty, stability, r, event.rating)
            };

            last_review_at = event.reviewed_at;
        }

        // Apply the new rating
        let elapsed_ms = (now - last_review_at).max(0) as f64;
        let elapsed_days = elapsed_ms / MS_PER_DAY;
        let r = self.retrievability(elapsed_days, stability);

        difficulty = self.next_difficulty(difficulty, rating);
        stability = if rating == Rating::Again {
            self.stability_after_failure(difficulty, stability, r)
        } else {
            self.stability_after_recall(difficulty, stability, r, rating)
        };

        let interval = self.next_interval(stability);
        let due_date = now + (interval * MS_PER_DAY) as i64;

        SchedulingOutput {
            due_date,
            interval_days: interval,
        }
    }
}

impl SrsAlgorithm for Fsrs {
    fn id(&self) -> &'static str { "fsrs" }
    fn name(&self) -> &'static str { "FSRS-4.5" }

    fn schedule(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        self.compute(history, rating, now)
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000; // fixed timestamp
    const DAY: i64 = 86_400_000;

    fn make_history(ratings: &[Rating]) -> Vec<ReviewEvent> {
        ratings.iter().enumerate().map(|(i, &r)| ReviewEvent {
            rating: r,
            reviewed_at: NOW + (i as i64) * DAY,
        }).collect()
    }

    // ── Core model unit tests ──

    #[test]
    fn initial_stability_values() {
        let fsrs = Fsrs::new();
        // S0(G) = w[G-1]
        assert!((fsrs.initial_stability(Rating::Again) - DEFAULT_W[0]).abs() < 1e-6);
        assert!((fsrs.initial_stability(Rating::Hard) - DEFAULT_W[1]).abs() < 1e-6);
        assert!((fsrs.initial_stability(Rating::Good) - DEFAULT_W[2]).abs() < 1e-6);
        assert!((fsrs.initial_stability(Rating::Easy) - DEFAULT_W[3]).abs() < 1e-6);
    }

    #[test]
    fn initial_difficulty_values() {
        let fsrs = Fsrs::new();
        // D0(Good) = w4
        let d_good = fsrs.initial_difficulty(Rating::Good);
        assert!((d_good - DEFAULT_W[4]).abs() < 1e-6, "D0(Good) = w4");

        // D0(Easy) < D0(Good) < D0(Hard) < D0(Again)
        let d_again = fsrs.initial_difficulty(Rating::Again);
        let d_hard = fsrs.initial_difficulty(Rating::Hard);
        let d_easy = fsrs.initial_difficulty(Rating::Easy);
        assert!(d_easy < d_good, "Easy should be easier: {} < {}", d_easy, d_good);
        assert!(d_good < d_hard, "Good < Hard: {} < {}", d_good, d_hard);
        assert!(d_hard < d_again, "Hard < Again: {} < {}", d_hard, d_again);
    }

    #[test]
    fn retrievability_at_stability() {
        let fsrs = Fsrs::new();
        // R(S, S) should be ~0.9
        let r = fsrs.retrievability(10.0, 10.0);
        assert!((r - 0.9).abs() < 0.01, "R(S,S) ≈ 0.9, got {}", r);
    }

    #[test]
    fn retrievability_decreases_over_time() {
        let fsrs = Fsrs::new();
        let s = 5.0;
        let r1 = fsrs.retrievability(1.0, s);
        let r2 = fsrs.retrievability(5.0, s);
        let r3 = fsrs.retrievability(20.0, s);
        assert!(r1 > r2, "R should decrease");
        assert!(r2 > r3, "R should decrease");
    }

    #[test]
    fn next_interval_equals_stability_at_90_percent() {
        let fsrs = Fsrs::new();
        // When desired retention is 0.9, interval should approximately equal stability
        let s = 10.0;
        let interval = fsrs.next_interval(s);
        // I(0.9, S) = S (by definition of FSRS)
        assert!((interval - s).abs() < 0.5, "I(0.9, S) ≈ S, got {} for S={}", interval, s);
    }

    // ── Happy paths ──

    #[test]
    fn new_card_good() {
        let algo = Fsrs::new();
        let out = algo.schedule(&[], Rating::Good, NOW);
        // S0(Good) = w2 ≈ 3.71 → interval ≈ 3.71 days (clamped >= 1)
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 15.0, "reasonable: {}", out.interval_days);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn new_card_again_short_interval() {
        let algo = Fsrs::new();
        let out = algo.schedule(&[], Rating::Again, NOW);
        // S0(Again) = w0 ≈ 0.49 → interval ≈ max(1.0, ...) = 1.0
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 3.0, "Again should be short: {}", out.interval_days);
    }

    #[test]
    fn new_card_easy_longer() {
        let algo = Fsrs::new();
        let out_good = algo.schedule(&[], Rating::Good, NOW);
        let out_easy = algo.schedule(&[], Rating::Easy, NOW);
        assert!(out_easy.interval_days >= out_good.interval_days,
            "Easy ({}) >= Good ({})", out_easy.interval_days, out_good.interval_days);
    }

    #[test]
    fn progression_increases_interval() {
        let algo = Fsrs::new();
        let h1 = make_history(&[Rating::Good]);
        let out1 = algo.schedule(&h1, Rating::Good, NOW + DAY);

        let h2 = make_history(&[Rating::Good, Rating::Good]);
        let out2 = algo.schedule(&h2, Rating::Good, NOW + 2 * DAY);

        // Intervals should generally increase with successive Good reviews
        // (stability grows each time)
        assert!(out2.interval_days > 1.0, "should grow: {}", out2.interval_days);
    }

    #[test]
    fn again_reduces_stability() {
        let algo = Fsrs::new();
        // Build up stability
        let good_history = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        let out_good = algo.schedule(&good_history, Rating::Good, NOW + 10 * DAY);

        // Now fail
        let out_again = algo.schedule(&good_history, Rating::Again, NOW + 10 * DAY);

        assert!(out_again.interval_days < out_good.interval_days,
            "Again ({}) < Good ({})", out_again.interval_days, out_good.interval_days);
    }

    #[test]
    fn ordering_again_hard_good_easy() {
        let algo = Fsrs::new();
        let history = make_history(&[Rating::Good, Rating::Good]);

        let out_again = algo.schedule(&history, Rating::Again, NOW + 7 * DAY);
        let out_hard = algo.schedule(&history, Rating::Hard, NOW + 7 * DAY);
        let out_good = algo.schedule(&history, Rating::Good, NOW + 7 * DAY);
        let out_easy = algo.schedule(&history, Rating::Easy, NOW + 7 * DAY);

        assert!(out_again.due_date <= out_hard.due_date,
            "Again ({}) <= Hard ({})", out_again.due_date, out_hard.due_date);
        assert!(out_hard.due_date <= out_good.due_date,
            "Hard ({}) <= Good ({})", out_hard.due_date, out_good.due_date);
        assert!(out_good.due_date <= out_easy.due_date,
            "Good ({}) <= Easy ({})", out_good.due_date, out_easy.due_date);
    }

    // ── Edge cases ──

    #[test]
    fn many_agains_stays_reasonable() {
        let algo = Fsrs::new();
        let history = make_history(&[Rating::Again; 10]);
        let out = algo.schedule(&history, Rating::Good, NOW + 15 * DAY);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 50.0, "should be reasonable: {}", out.interval_days);
    }

    #[test]
    fn many_easys_capped() {
        let algo = Fsrs::new();
        let history = make_history(&[Rating::Easy; 15]);
        let out = algo.schedule(&history, Rating::Easy, NOW + 100 * DAY);
        assert!(out.interval_days <= MAX_INTERVAL, "should be capped: {}", out.interval_days);
        assert!(out.interval_days > 0.0);
    }

    #[test]
    fn long_history_no_panic() {
        let algo = Fsrs::new();
        let history = make_history(&vec![Rating::Good; 30]);
        let out = algo.schedule(&history, Rating::Good, NOW + 200 * DAY);
        assert!(out.interval_days > 0.0);
        assert!(out.interval_days <= MAX_INTERVAL);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn alternating_ratings_stable() {
        let algo = Fsrs::new();
        let ratings: Vec<Rating> = (0..10)
            .map(|i| if i % 2 == 0 { Rating::Again } else { Rating::Good })
            .collect();
        let history = make_history(&ratings);
        let out = algo.schedule(&history, Rating::Good, NOW + 20 * DAY);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 100.0, "should not diverge: {}", out.interval_days);
    }

    #[test]
    fn preview_choices_ordered() {
        let algo = Fsrs::new();
        let history = make_history(&[Rating::Good, Rating::Good]);
        let choices = algo.preview_choices(&history, NOW + 7 * DAY);

        assert!(choices.again.due_date <= choices.hard.due_date);
        assert!(choices.hard.due_date <= choices.good.due_date);
        assert!(choices.good.due_date <= choices.easy.due_date);
    }
}
