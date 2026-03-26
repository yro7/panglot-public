//! Shared morphological enums for cross-language reuse.
//!
//! **Semantic bijection principle**: An enum belongs here only if its variants
//! carry the *same meaning* across every language that uses it. "Masculine" in
//! Arabic is the same concept as "Masculine" in Russian — that is a true
//! bijection. By contrast, tenses, moods, and cases are language-specific
//! systems with no universal mapping and therefore stay in each language module.

use serde::{Deserialize, Serialize};

// Person

/// Grammatical person
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Person {
    First,
    Second,
    Third,
}

// Number

/// Binary grammatical number (singular / plural).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BinaryNumber {
    Singular,
    Plural,
}

/// Ternary grammatical number (singular / dual / plural).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TernaryNumber {
    Singular,
    Dual,
    Plural,
}

// Gender

/// Two-gender system (masculine / feminine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BinaryGender {
    Masculine,
    Feminine,
}

/// Three-gender system (masculine / feminine / neuter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TernaryGender {
    Masculine,
    Feminine,
    Neuter,
}

// Aspect

/// Slavic verbal aspect (perfective / imperfective).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlavicAspect {
    Perfective,
    Imperfective,
}

// Voice

/// Binary voice (active / passive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BinaryVoice {
    Active,
    Passive,
}
