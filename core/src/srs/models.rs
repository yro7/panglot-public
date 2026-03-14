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
