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
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }

    /// Parse from string (case-insensitive). Returns `Again` for unrecognized values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "hard"  | "2" => Self::Hard,
            "good"  | "3" => Self::Good,
            "easy"  | "4" => Self::Easy,
            _ => Self::Again,
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
