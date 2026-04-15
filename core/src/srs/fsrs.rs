//! FSRS implementations: FSRS-4.5, FSRS-5, FSRS-6.
//!
//! Reference: https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm
//!
//! All versions share the DSR (Difficulty, Stability, Retrievability) memory model:
//!   - Stability (S): days until retrievability drops to 90%
//!   - Difficulty (D): inherent difficulty ∈ [1, 10]
//!   - Retrievability (R): probability of recall at time t
//!
//! Ratings / grades:
//!   Again=1, Hard=2, Good=3, Easy=4

use super::{Rating, ReviewEvent, SchedulingOutput, SrsAlgorithm};

const MS_PER_DAY: f64 = 86_400_000.0;
const DESIRED_RETENTION: f64 = 0.9;
const MAX_INTERVAL: f64 = 365.0;

/// Elapsed days threshold for "same-day" (short-term) reviews.
/// Learning steps are 1 min / 10 min — both well below 1 day.
const SHORT_TERM_THRESHOLD_DAYS: f64 = 1.0;


// ═══════════════════════════════════════════════════════════════
// FSRS-4.5  —  17 parameters
// ═══════════════════════════════════════════════════════════════

/// FSRS-4.5 forgetting curve constants.
/// DECAY = -0.5, FACTOR = 19/81  →  ensures R(S, S) = 0.9.
const DECAY_45: f64 = -0.5;
const FACTOR_45: f64 = 19.0 / 81.0;

/// Default FSRS-4.5 parameters from the official wiki.
const DEFAULT_W_45: [f64; 17] = [
    0.4872,  // w0:  S0(Again)
    1.4003,  // w1:  S0(Hard)
    3.7145,  // w2:  S0(Good)
    13.8206, // w3:  S0(Easy)
    5.1618,  // w4:  D0(Good)   [mean-reversion target = D0(3) = w4]
    1.2298,  // w5:  D0 linear scaling
    0.8975,  // w6:  difficulty update step
    0.031,   // w7:  mean-reversion weight
    1.6474,  // w8:  Sr exponent (base)
    0.1367,  // w9:  Sr stability power
    1.0461,  // w10: Sr retrievability exponent
    2.1072,  // w11: Sf base
    0.0793,  // w12: Sf difficulty power
    0.3246,  // w13: Sf stability power
    1.587,   // w14: Sf retrievability exponent
    0.2272,  // w15: Hard penalty
    2.8755,  // w16: Easy bonus
];

pub struct Fsrs {
    w: [f64; 17],
}

impl Default for Fsrs {
    fn default() -> Self {
        Self { w: DEFAULT_W_45 }
    }
}

impl Fsrs {
    pub fn new() -> Self {
        Self::default()
    }

    /// S0(G) = w[G-1]
    fn initial_stability(&self, rating: Rating) -> f64 {
        self.w[(rating as u8 - 1) as usize].max(0.1)
    }

    /// D0(G) = w4 − (G−3)·w5,  clamped [1, 10]
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        (self.w[4] - (g - 3.0) * self.w[5]).clamp(1.0, 10.0)
    }

    /// D′(D,G) = w7·D0(3) + (1−w7)·(D − w6·(G−3)),  clamped [1, 10]
    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let d_prime = d - self.w[6] * (g - 3.0);
        let d0_good = self.w[4]; // D0(3) = w4
        (self.w[7] * d0_good + (1.0 - self.w[7]) * d_prime).clamp(1.0, 10.0)
    }

    /// R(t,S) = (1 + FACTOR·t/S)^DECAY
    fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        (1.0 + FACTOR_45 * elapsed_days / stability).powf(DECAY_45)
    }

    /// I(r,S) = (S/FACTOR)·(r^(1/DECAY) − 1)
    fn next_interval(&self, stability: f64) -> f64 {
        let interval =
            (stability / FACTOR_45) * (DESIRED_RETENTION.powf(1.0 / DECAY_45) - 1.0);
        interval.clamp(1.0, MAX_INTERVAL)
    }

    /// S′r(D,S,R,G) = S·(e^w8·(11−D)·S^(−w9)·(e^(w10·(1−R))−1)·penalty·bonus + 1),  SInc≥1
    fn stability_after_recall(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard { self.w[15] } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { self.w[16] } else { 1.0 };
        let s_inc = (self.w[8].exp()
            * (11.0 - d)
            * s.powf(-self.w[9])
            * ((self.w[10] * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus)
            .max(1.0); // SInc ≥ 1 for successful recall
        s * s_inc
    }

    /// S′f(D,S,R) = w11·D^(−w12)·((S+1)^w13−1)·e^(w14·(1−R))
    fn stability_after_failure(&self, d: f64, s: f64, r: f64) -> f64 {
        let new_s = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * (self.w[14] * (1.0 - r)).exp();
        new_s.max(0.1).min(s)
    }

    fn compute_internal(
        &self,
        history: &[ReviewEvent],
        rating: Rating,
        now: i64,
    ) -> SchedulingOutput {
        if history.is_empty() {
            let s = self.initial_stability(rating);
            let interval = self.next_interval(s);
            return SchedulingOutput {
                due_date: now + (interval * MS_PER_DAY) as i64,
                interval_days: interval,
            };
        }

        let mut s = self.initial_stability(history[0].rating);
        let mut d = self.initial_difficulty(history[0].rating);
        let mut last_reviewed_at = history[0].reviewed_at;

        for event in &history[1..] {
            let elapsed_days =
                (event.reviewed_at - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
            let r = self.retrievability(elapsed_days, s);

            d = self.next_difficulty(d, event.rating);
            s = if event.rating == Rating::Again {
                self.stability_after_failure(d, s, r)
            } else {
                self.stability_after_recall(d, s, r, event.rating)
            };

            last_reviewed_at = event.reviewed_at;
        }

        let elapsed_days = (now - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
        let r = self.retrievability(elapsed_days, s);

        d = self.next_difficulty(d, rating);
        s = if rating == Rating::Again {
            self.stability_after_failure(d, s, r)
        } else {
            self.stability_after_recall(d, s, r, rating)
        };

        let interval = self.next_interval(s);
        SchedulingOutput {
            due_date: now + (interval * MS_PER_DAY) as i64,
            interval_days: interval,
        }
    }
}

impl SrsAlgorithm for Fsrs {
    fn id(&self) -> &'static str {
        "fsrs-4.5"
    }
    fn name(&self) -> &'static str {
        "FSRS-4.5"
    }
    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        self.compute_internal(history, rating, now)
    }
}


// ═══════════════════════════════════════════════════════════════
// FSRS-5  —  19 parameters
// ═══════════════════════════════════════════════════════════════
//
// Changes vs FSRS-4.5:
//   • D0(G) = w4 − e^(w5·(G−1)) + 1  (w4 = D0(1), i.e. initial difficulty for Again)
//   • Linear damping for D update:
//       ΔD(G) = −w6·(G−3)
//       D′ = D + ΔD·(10−D)/9
//   • Mean-reversion target is D0(4) not D0(3):
//       D″ = w7·D0(4) + (1−w7)·D′
//   • Short-term stability (same-day review, elapsed < 1 day):
//       S′(S,G) = S·e^(w17·(G−3+w18))
//   • Forgetting curve: same as FSRS-4.5 (DECAY=−0.5, FACTOR=19/81)
//   • Sr and Sf formulas: same as FSRS-4.5

const DEFAULT_W_5: [f64; 19] = [
    0.40255,  // w0:  S0(Again)
    1.18385,  // w1:  S0(Hard)
    3.173,    // w2:  S0(Good)
    15.69105, // w3:  S0(Easy)
    7.1949,   // w4:  D0(1) = D0(Again)
    0.5345,   // w5:  D0 exponential factor
    1.4604,   // w6:  linear damping step
    0.0046,   // w7:  mean-reversion weight
    1.54575,  // w8:  Sr exponent (base)
    0.1192,   // w9:  Sr stability power
    1.01925,  // w10: Sr retrievability exponent
    1.9395,   // w11: Sf base
    0.11,     // w12: Sf difficulty power
    0.29605,  // w13: Sf stability power
    2.2698,   // w14: Sf retrievability exponent
    0.2315,   // w15: Hard penalty
    2.9898,   // w16: Easy bonus
    0.51655,  // w17: short-term stability exponent
    0.6621,   // w18: short-term stability offset
];

pub struct Fsrs5 {
    w: [f64; 19],
}

impl Default for Fsrs5 {
    fn default() -> Self {
        Self { w: DEFAULT_W_5 }
    }
}

impl Fsrs5 {
    pub fn new() -> Self {
        Self::default()
    }

    /// S0(G) = w[G-1]
    fn initial_stability(&self, rating: Rating) -> f64 {
        self.w[(rating as u8 - 1) as usize].max(0.1)
    }

    /// D0(G) = w4 − e^(w5·(G−1)) + 1,  w4 = D0(1),  clamped [1, 10]
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        (self.w[4] - (self.w[5] * (g - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
    }

    /// D0(4) — the mean-reversion target in FSRS-5.
    fn d0_easy(&self) -> f64 {
        (self.w[4] - (self.w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0)
    }

    /// Linear damping + mean reversion toward D0(4).
    ///   ΔD(G) = −w6·(G−3)
    ///   D′  = D + ΔD·(10−D)/9
    ///   D″  = w7·D0(4) + (1−w7)·D′,  clamped [1, 10]
    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let delta_d = -self.w[6] * (g - 3.0);
        let d_prime = d + delta_d * (10.0 - d) / 9.0;
        (self.w[7] * self.d0_easy() + (1.0 - self.w[7]) * d_prime).clamp(1.0, 10.0)
    }

    /// R(t,S) = (1 + FACTOR_45·t/S)^DECAY_45   (same as FSRS-4.5)
    fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        (1.0 + FACTOR_45 * elapsed_days / stability).powf(DECAY_45)
    }

    /// I(r,S) using FSRS-4.5 constants  (same as FSRS-4.5)
    fn next_interval(&self, stability: f64) -> f64 {
        let interval =
            (stability / FACTOR_45) * (DESIRED_RETENTION.powf(1.0 / DECAY_45) - 1.0);
        interval.clamp(1.0, MAX_INTERVAL)
    }

    /// Short-term (same-day) stability:  S′ = S·e^(w17·(G−3+w18))
    fn stability_short_term(&self, s: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        (s * (self.w[17] * (g - 3.0 + self.w[18])).exp()).max(0.1)
    }

    /// S′r — same formula as FSRS-4.5.
    fn stability_after_recall(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard { self.w[15] } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { self.w[16] } else { 1.0 };
        let s_inc = (self.w[8].exp()
            * (11.0 - d)
            * s.powf(-self.w[9])
            * ((self.w[10] * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus)
            .max(1.0);
        s * s_inc
    }

    /// S′f — same formula as FSRS-4.5.
    fn stability_after_failure(&self, d: f64, s: f64, r: f64) -> f64 {
        let new_s = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * (self.w[14] * (1.0 - r)).exp();
        new_s.max(0.1).min(s)
    }

    fn compute_internal(
        &self,
        history: &[ReviewEvent],
        rating: Rating,
        now: i64,
    ) -> SchedulingOutput {
        if history.is_empty() {
            let s = self.initial_stability(rating);
            let interval = self.next_interval(s);
            return SchedulingOutput {
                due_date: now + (interval * MS_PER_DAY) as i64,
                interval_days: interval,
            };
        }

        let mut s = self.initial_stability(history[0].rating);
        let mut d = self.initial_difficulty(history[0].rating);
        let mut last_reviewed_at = history[0].reviewed_at;

        for event in &history[1..] {
            let elapsed_days =
                (event.reviewed_at - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
            let r = self.retrievability(elapsed_days, s);

            if elapsed_days < SHORT_TERM_THRESHOLD_DAYS {
                // Same-day review: update stability only (D is unchanged)
                s = if event.rating == Rating::Again {
                    self.stability_after_failure(d, s, r)
                } else {
                    self.stability_short_term(s, event.rating)
                };
            } else {
                d = self.next_difficulty(d, event.rating);
                s = if event.rating == Rating::Again {
                    self.stability_after_failure(d, s, r)
                } else {
                    self.stability_after_recall(d, s, r, event.rating)
                };
            }

            last_reviewed_at = event.reviewed_at;
        }

        let elapsed_days = (now - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
        let r = self.retrievability(elapsed_days, s);

        if elapsed_days < SHORT_TERM_THRESHOLD_DAYS {
            s = if rating == Rating::Again {
                self.stability_after_failure(d, s, r)
            } else {
                self.stability_short_term(s, rating)
            };
        } else {
            d = self.next_difficulty(d, rating);
            s = if rating == Rating::Again {
                self.stability_after_failure(d, s, r)
            } else {
                self.stability_after_recall(d, s, r, rating)
            };
        }

        let interval = self.next_interval(s);
        SchedulingOutput {
            due_date: now + (interval * MS_PER_DAY) as i64,
            interval_days: interval,
        }
    }
}

impl SrsAlgorithm for Fsrs5 {
    fn id(&self) -> &'static str {
        "fsrs-5"
    }
    fn name(&self) -> &'static str {
        "FSRS-5"
    }
    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        self.compute_internal(history, rating, now)
    }
}


// ═══════════════════════════════════════════════════════════════
// FSRS-6  —  21 parameters
// ═══════════════════════════════════════════════════════════════
//
// Changes vs FSRS-5:
//   • Short-term stability with stability dampening:
//       SInc = e^(w17·(G−3+w18)) · S^(−w19)
//       S′ = S · SInc   (SInc ≥ 1 enforced when G ≥ Good)
//   • Trainable forgetting-curve decay (w20):
//       DECAY  = −w20
//       factor = 0.9^(−1/w20) − 1    ensures R(S,S) = 90%
//       R(t,S) = (1 + factor·t/S)^(−w20)
//   • D0, D′, Sr, Sf: same formulas as FSRS-5

const DEFAULT_W_6: [f64; 21] = [
    0.212,   // w0:  S0(Again)
    1.2931,  // w1:  S0(Hard)
    2.3065,  // w2:  S0(Good)
    8.2956,  // w3:  S0(Easy)
    6.4133,  // w4:  D0(1) = D0(Again)
    0.8334,  // w5:  D0 exponential factor
    3.0194,  // w6:  linear damping step
    0.001,   // w7:  mean-reversion weight
    1.8722,  // w8:  Sr exponent (base)
    0.1666,  // w9:  Sr stability power
    0.796,   // w10: Sr retrievability exponent
    1.4835,  // w11: Sf base
    0.0614,  // w12: Sf difficulty power
    0.2629,  // w13: Sf stability power
    1.6483,  // w14: Sf retrievability exponent
    0.6014,  // w15: Hard penalty
    1.8729,  // w16: Easy bonus
    0.5425,  // w17: short-term stability exponent
    0.0912,  // w18: short-term stability offset
    0.0658,  // w19: short-term S dampening (S^(−w19))
    0.1542,  // w20: trainable DECAY magnitude  (DECAY = −w20)
];

pub struct Fsrs6 {
    w: [f64; 21],
}

impl Default for Fsrs6 {
    fn default() -> Self {
        Self { w: DEFAULT_W_6 }
    }
}

impl Fsrs6 {
    pub fn new() -> Self {
        Self::default()
    }

    /// S0(G) = w[G-1]
    fn initial_stability(&self, rating: Rating) -> f64 {
        self.w[(rating as u8 - 1) as usize].max(0.1)
    }

    /// D0(G) = w4 − e^(w5·(G−1)) + 1,  same formula as FSRS-5.
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        (self.w[4] - (self.w[5] * (g - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
    }

    /// D0(4) — mean-reversion target.
    fn d0_easy(&self) -> f64 {
        (self.w[4] - (self.w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0)
    }

    /// Same linear damping + mean-reversion as FSRS-5.
    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let delta_d = -self.w[6] * (g - 3.0);
        let d_prime = d + delta_d * (10.0 - d) / 9.0;
        (self.w[7] * self.d0_easy() + (1.0 - self.w[7]) * d_prime).clamp(1.0, 10.0)
    }

    /// Trainable DECAY = −w20.
    fn decay(&self) -> f64 {
        -self.w[20]
    }

    /// factor = 0.9^(−1/w20) − 1   ensures R(S,S) = 90%.
    fn factor(&self) -> f64 {
        0.9_f64.powf(-1.0 / self.w[20]) - 1.0
    }

    /// R(t,S) = (1 + factor·t/S)^DECAY
    fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        (1.0 + self.factor() * elapsed_days / stability).powf(self.decay())
    }

    /// I(r,S) using trainable DECAY.
    fn next_interval(&self, stability: f64) -> f64 {
        let decay = self.decay();
        let factor = self.factor();
        let interval = (stability / factor) * (DESIRED_RETENTION.powf(1.0 / decay) - 1.0);
        interval.clamp(1.0, MAX_INTERVAL)
    }

    /// Short-term stability with S-dampening:
    ///   SInc = e^(w17·(G−3+w18)) · S^(−w19)
    ///   SInc ≥ 1 enforced for Good and Easy.
    fn stability_short_term(&self, s: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let s_inc_raw = (self.w[17] * (g - 3.0 + self.w[18])).exp() * s.powf(-self.w[19]);
        let s_inc = match rating {
            Rating::Good | Rating::Easy => s_inc_raw.max(1.0),
            _ => s_inc_raw,
        };
        (s * s_inc).max(0.1)
    }

    /// S′r — same as FSRS-4.5 / FSRS-5.
    fn stability_after_recall(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard { self.w[15] } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { self.w[16] } else { 1.0 };
        let s_inc = (self.w[8].exp()
            * (11.0 - d)
            * s.powf(-self.w[9])
            * ((self.w[10] * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus)
            .max(1.0);
        s * s_inc
    }

    /// S′f — same as FSRS-4.5 / FSRS-5.
    fn stability_after_failure(&self, d: f64, s: f64, r: f64) -> f64 {
        let new_s = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * (self.w[14] * (1.0 - r)).exp();
        new_s.max(0.1).min(s)
    }

    fn compute_internal(
        &self,
        history: &[ReviewEvent],
        rating: Rating,
        now: i64,
    ) -> SchedulingOutput {
        if history.is_empty() {
            let s = self.initial_stability(rating);
            let interval = self.next_interval(s);
            return SchedulingOutput {
                due_date: now + (interval * MS_PER_DAY) as i64,
                interval_days: interval,
            };
        }

        let mut s = self.initial_stability(history[0].rating);
        let mut d = self.initial_difficulty(history[0].rating);
        let mut last_reviewed_at = history[0].reviewed_at;

        for event in &history[1..] {
            let elapsed_days =
                (event.reviewed_at - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
            let r = self.retrievability(elapsed_days, s);

            if elapsed_days < SHORT_TERM_THRESHOLD_DAYS {
                s = if event.rating == Rating::Again {
                    self.stability_after_failure(d, s, r)
                } else {
                    self.stability_short_term(s, event.rating)
                };
            } else {
                d = self.next_difficulty(d, event.rating);
                s = if event.rating == Rating::Again {
                    self.stability_after_failure(d, s, r)
                } else {
                    self.stability_after_recall(d, s, r, event.rating)
                };
            }

            last_reviewed_at = event.reviewed_at;
        }

        let elapsed_days = (now - last_reviewed_at).max(0) as f64 / MS_PER_DAY;
        let r = self.retrievability(elapsed_days, s);

        if elapsed_days < SHORT_TERM_THRESHOLD_DAYS {
            s = if rating == Rating::Again {
                self.stability_after_failure(d, s, r)
            } else {
                self.stability_short_term(s, rating)
            };
        } else {
            d = self.next_difficulty(d, rating);
            s = if rating == Rating::Again {
                self.stability_after_failure(d, s, r)
            } else {
                self.stability_after_recall(d, s, r, rating)
            };
        }

        let interval = self.next_interval(s);
        SchedulingOutput {
            due_date: now + (interval * MS_PER_DAY) as i64,
            interval_days: interval,
        }
    }
}

impl SrsAlgorithm for Fsrs6 {
    fn id(&self) -> &'static str {
        "fsrs-6"
    }
    fn name(&self) -> &'static str {
        "FSRS-6"
    }
    fn compute(&self, history: &[ReviewEvent], rating: Rating, now: i64) -> SchedulingOutput {
        self.compute_internal(history, rating, now)
    }
}


// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;
    const DAY: i64 = 86_400_000;
    const MIN: i64 = 60_000;

    fn make_history(ratings: &[Rating]) -> Vec<ReviewEvent> {
        ratings
            .iter()
            .enumerate()
            .map(|(i, &r)| ReviewEvent {
                rating: r,
                reviewed_at: NOW + (i as i64) * DAY,
            })
            .collect()
    }

    /// History with same-day reviews (learning steps: 1 min, 10 min).
    fn make_learning_history(ratings: &[Rating]) -> Vec<ReviewEvent> {
        let steps: Vec<i64> = vec![0, MIN, 10 * MIN];
        ratings
            .iter()
            .enumerate()
            .map(|(i, &r)| ReviewEvent {
                rating: r,
                reviewed_at: NOW + steps.get(i).copied().unwrap_or((i as i64) * DAY),
            })
            .collect()
    }

    // ── FSRS-4.5 ──

    #[test]
    fn fsrs45_r_at_stability_is_90pct() {
        let algo = Fsrs::new();
        let r = algo.retrievability(10.0, 10.0);
        assert!((r - 0.9).abs() < 0.01, "R(S,S) ≈ 0.9, got {}", r);
    }

    #[test]
    fn fsrs45_new_card_good() {
        let algo = Fsrs::new();
        let out = algo.compute(&[], Rating::Good, NOW);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 20.0, "got {}", out.interval_days);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn fsrs45_ordering() {
        let algo = Fsrs::new();
        let h = make_history(&[Rating::Good, Rating::Good]);
        let now = NOW + 7 * DAY;
        let again = algo.compute(&h, Rating::Again, now);
        let hard = algo.compute(&h, Rating::Hard, now);
        let good = algo.compute(&h, Rating::Good, now);
        let easy = algo.compute(&h, Rating::Easy, now);
        assert!(again.due_date <= hard.due_date, "Again ≤ Hard");
        assert!(hard.due_date <= good.due_date, "Hard ≤ Good");
        assert!(good.due_date <= easy.due_date, "Good ≤ Easy");
    }

    #[test]
    fn fsrs45_again_reduces_interval() {
        let algo = Fsrs::new();
        let h = make_history(&[Rating::Good, Rating::Good, Rating::Good]);
        let now = NOW + 10 * DAY;
        let good_out = algo.compute(&h, Rating::Good, now);
        let fail_out = algo.compute(&h, Rating::Again, now);
        assert!(
            fail_out.interval_days < good_out.interval_days,
            "Again ({}) < Good ({})",
            fail_out.interval_days,
            good_out.interval_days
        );
    }

    #[test]
    fn fsrs45_interval_capped() {
        let algo = Fsrs::new();
        let h = make_history(&[Rating::Easy; 15]);
        let out = algo.compute(&h, Rating::Easy, NOW + 100 * DAY);
        assert!(out.interval_days <= MAX_INTERVAL);
    }

    #[test]
    fn fsrs45_id_and_name() {
        let algo = Fsrs::new();
        assert_eq!(algo.id(), "fsrs-4.5");
        assert_eq!(algo.name(), "FSRS-4.5");
    }

    // ── FSRS-5 ──

    #[test]
    fn fsrs5_r_at_stability_is_90pct() {
        let algo = Fsrs5::new();
        let r = algo.retrievability(10.0, 10.0);
        assert!((r - 0.9).abs() < 0.01, "R(S,S) ≈ 0.9, got {}", r);
    }

    #[test]
    fn fsrs5_new_card_good() {
        let algo = Fsrs5::new();
        let out = algo.compute(&[], Rating::Good, NOW);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 20.0, "got {}", out.interval_days);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn fsrs5_ordering() {
        let algo = Fsrs5::new();
        let h = make_history(&[Rating::Good, Rating::Good]);
        let now = NOW + 7 * DAY;
        let again = algo.compute(&h, Rating::Again, now);
        let hard = algo.compute(&h, Rating::Hard, now);
        let good = algo.compute(&h, Rating::Good, now);
        let easy = algo.compute(&h, Rating::Easy, now);
        assert!(again.due_date <= hard.due_date, "Again ≤ Hard");
        assert!(hard.due_date <= good.due_date, "Hard ≤ Good");
        assert!(good.due_date <= easy.due_date, "Good ≤ Easy");
    }

    #[test]
    fn fsrs5_d0_difficulty_ordering() {
        let algo = Fsrs5::new();
        // D0: Again > Hard > Good > Easy  (more difficult = higher D)
        let d_again = algo.initial_difficulty(Rating::Again);
        let d_hard = algo.initial_difficulty(Rating::Hard);
        let d_good = algo.initial_difficulty(Rating::Good);
        let d_easy = algo.initial_difficulty(Rating::Easy);
        assert!(d_again > d_hard, "{} > {}", d_again, d_hard);
        assert!(d_hard > d_good, "{} > {}", d_hard, d_good);
        assert!(d_good > d_easy, "{} > {}", d_good, d_easy);
    }

    #[test]
    fn fsrs5_d0_again_equals_w4() {
        let algo = Fsrs5::new();
        // D0(1) = w4 - e^(w5*0) + 1 = w4 - 1 + 1 = w4
        let d_again = algo.initial_difficulty(Rating::Again);
        assert!((d_again - DEFAULT_W_5[4]).abs() < 1e-6, "D0(Again)=w4: {} vs {}", d_again, DEFAULT_W_5[4]);
    }

    #[test]
    fn fsrs5_short_term_stability() {
        let algo = Fsrs5::new();
        // Good review on same day should increase stability
        let s_before = 2.0;
        let s_after = algo.stability_short_term(s_before, Rating::Good);
        assert!(s_after >= s_before, "Good short-term should not shrink stability: {} vs {}", s_after, s_before);
    }

    #[test]
    fn fsrs5_same_day_history() {
        let algo = Fsrs5::new();
        // Learning sequence: Good then Good 10 min later — should still produce valid interval
        let h = make_learning_history(&[Rating::Good, Rating::Good]);
        let out = algo.compute(&h, Rating::Good, NOW + DAY);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 100.0, "got {}", out.interval_days);
    }

    #[test]
    fn fsrs5_interval_capped() {
        let algo = Fsrs5::new();
        let h = make_history(&[Rating::Easy; 15]);
        let out = algo.compute(&h, Rating::Easy, NOW + 100 * DAY);
        assert!(out.interval_days <= MAX_INTERVAL);
    }

    #[test]
    fn fsrs5_id_and_name() {
        let algo = Fsrs5::new();
        assert_eq!(algo.id(), "fsrs-5");
        assert_eq!(algo.name(), "FSRS-5");
    }

    // ── FSRS-6 ──

    #[test]
    fn fsrs6_r_at_stability_is_90pct() {
        let algo = Fsrs6::new();
        let r = algo.retrievability(10.0, 10.0);
        assert!((r - 0.9).abs() < 0.01, "R(S,S) ≈ 0.9, got {}", r);
    }

    #[test]
    fn fsrs6_new_card_good() {
        let algo = Fsrs6::new();
        let out = algo.compute(&[], Rating::Good, NOW);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 20.0, "got {}", out.interval_days);
        assert!(out.due_date > NOW);
    }

    #[test]
    fn fsrs6_ordering() {
        let algo = Fsrs6::new();
        let h = make_history(&[Rating::Good, Rating::Good]);
        let now = NOW + 7 * DAY;
        let again = algo.compute(&h, Rating::Again, now);
        let hard = algo.compute(&h, Rating::Hard, now);
        let good = algo.compute(&h, Rating::Good, now);
        let easy = algo.compute(&h, Rating::Easy, now);
        assert!(again.due_date <= hard.due_date, "Again ≤ Hard");
        assert!(hard.due_date <= good.due_date, "Hard ≤ Good");
        assert!(good.due_date <= easy.due_date, "Good ≤ Easy");
    }

    #[test]
    fn fsrs6_trainable_decay() {
        let algo = Fsrs6::new();
        // factor ensures R(S,S) = 0.9 for the trainable decay
        let r = algo.retrievability(10.0, 10.0);
        assert!((r - 0.9).abs() < 0.01, "trainable decay: R(S,S)≈0.9, got {}", r);
    }

    #[test]
    fn fsrs6_short_term_good_enforces_sinc_ge_1() {
        let algo = Fsrs6::new();
        // For a very stable card, SInc raw might be < 1, but we enforce >= 1 for Good
        let s_large = 200.0;
        let s_after = algo.stability_short_term(s_large, Rating::Good);
        assert!(s_after >= s_large, "Good short-term must not shrink stability for Good: {} vs {}", s_after, s_large);
    }

    #[test]
    fn fsrs6_same_day_history() {
        let algo = Fsrs6::new();
        let h = make_learning_history(&[Rating::Good, Rating::Good]);
        let out = algo.compute(&h, Rating::Good, NOW + DAY);
        assert!(out.interval_days >= 1.0);
        assert!(out.interval_days < 100.0, "got {}", out.interval_days);
    }

    #[test]
    fn fsrs6_interval_capped() {
        let algo = Fsrs6::new();
        let h = make_history(&[Rating::Easy; 15]);
        let out = algo.compute(&h, Rating::Easy, NOW + 100 * DAY);
        assert!(out.interval_days <= MAX_INTERVAL);
    }

    #[test]
    fn fsrs6_id_and_name() {
        let algo = Fsrs6::new();
        assert_eq!(algo.id(), "fsrs-6");
        assert_eq!(algo.name(), "FSRS-6");
    }
}
