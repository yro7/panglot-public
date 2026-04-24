use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ── Rating ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
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
            "hard" | "2" => Self::Hard,
            "good" | "3" => Self::Good,
            "easy" | "4" => Self::Easy,
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

#[derive(Debug, Clone)]
pub struct SchedulingChoices {
    pub again: SchedulingOutput,
    pub hard: SchedulingOutput,
    pub good: SchedulingOutput,
    pub easy: SchedulingOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SrsAlgorithmId {
    #[serde(rename = "sm2")]
    Sm2,
    #[serde(rename = "leitner")]
    Leitner,
    #[serde(rename = "fsrs-4.5")]
    Fsrs45,
    #[serde(rename = "fsrs-5")]
    Fsrs5,
    #[serde(rename = "fsrs-6")]
    Fsrs6,
}

impl SrsAlgorithmId {
    pub const ALL: [Self; 5] = [
        Self::Sm2,
        Self::Leitner,
        Self::Fsrs45,
        Self::Fsrs5,
        Self::Fsrs6,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sm2 => "sm2",
            Self::Leitner => "leitner",
            Self::Fsrs45 => "fsrs-4.5",
            Self::Fsrs5 => "fsrs-5",
            Self::Fsrs6 => "fsrs-6",
        }
    }
}

impl Default for SrsAlgorithmId {
    fn default() -> Self {
        Self::Sm2
    }
}

impl fmt::Display for SrsAlgorithmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SrsAlgorithmId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sm2" => Ok(Self::Sm2),
            "leitner" => Ok(Self::Leitner),
            "fsrs-4.5" => Ok(Self::Fsrs45),
            "fsrs-5" => Ok(Self::Fsrs5),
            "fsrs-6" => Ok(Self::Fsrs6),
            _ => Err(format!("Unsupported SRS algorithm id '{}'", value)),
        }
    }
}
