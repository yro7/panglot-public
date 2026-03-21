use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ── Bounded integer macro ──

macro_rules! bounded_int {
    ($name:ident, $inner:ty, $min:expr, $max:expr, $label:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name($inner);

        impl $name {
            pub const MIN: $inner = $min;
            pub const MAX: $inner = $max;

            pub fn new(value: $inner) -> Result<Self, String> {
                if value < Self::MIN || value > Self::MAX {
                    Err(format!(
                        "{}: value {} is out of range {}..={}",
                        $label, value, Self::MIN, Self::MAX
                    ))
                } else {
                    Ok(Self(value))
                }
            }

            pub fn get(&self) -> $inner {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <$inner>::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

// ── Bounded string macro ──

macro_rules! bounded_string {
    ($name:ident, $max:expr, $label:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            pub const MAX_LEN: usize = $max;

            pub fn new(value: String) -> Result<Self, String> {
                if value.len() > Self::MAX_LEN {
                    Err(format!(
                        "{}: length {} exceeds maximum of {} bytes",
                        $label,
                        value.len(),
                        Self::MAX_LEN
                    ))
                } else {
                    Ok(Self(value))
                }
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

// ── Concrete types ──

bounded_int!(CardCount, u32, 1, 20, "card_count");
bounded_int!(Difficulty, u8, 0, 10, "difficulty");
bounded_int!(LearnAheadMinutes, i32, 0, 1440, "learn_ahead_minutes");

bounded_string!(UserPrompt, 2_000, "user_prompt");
bounded_string!(NodeName, 200, "node_name");
bounded_string!(NodeInstructions, 5_000, "node_instructions");

// ── OpenAPI schema implementations (utoipa v5) ──

macro_rules! int_schema {
    ($name:ident, $desc:expr) => {
        impl utoipa::PartialSchema for $name {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::Type(
                        utoipa::openapi::schema::Type::Integer,
                    ))
                    .minimum(Some($name::MIN as f64))
                    .maximum(Some($name::MAX as f64))
                    .description(Some($desc))
                    .build()
                    .into()
            }
        }
        impl utoipa::ToSchema for $name {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
        }
    };
}

macro_rules! string_schema {
    ($name:ident, $desc:expr) => {
        impl utoipa::PartialSchema for $name {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::Type(
                        utoipa::openapi::schema::Type::String,
                    ))
                    .max_length(Some($name::MAX_LEN))
                    .description(Some($desc))
                    .build()
                    .into()
            }
        }
        impl utoipa::ToSchema for $name {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
        }
    };
}

int_schema!(CardCount, "Number of cards to generate (1-20)");
int_schema!(Difficulty, "Exercise difficulty level (0-10)");
int_schema!(LearnAheadMinutes, "Learn-ahead window in minutes (0-1440)");

string_schema!(UserPrompt, "User-provided prompt for card generation (max 2000 chars)");
string_schema!(NodeName, "Skill tree node name (max 200 chars)");
string_schema!(NodeInstructions, "Pedagogical instructions for a skill node (max 5000 chars)");

// ── Default for LearnAheadMinutes (DB backward compat) ──

impl Default for LearnAheadMinutes {
    fn default() -> Self {
        Self(20)
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_count_valid_range() {
        assert!(CardCount::new(1).is_ok());
        assert!(CardCount::new(10).is_ok());
        assert!(CardCount::new(20).is_ok());
    }

    #[test]
    fn card_count_rejects_out_of_range() {
        assert!(CardCount::new(0).is_err());
        assert!(CardCount::new(21).is_err());
        assert!(CardCount::new(u32::MAX).is_err());
    }

    #[test]
    fn difficulty_valid_range() {
        assert!(Difficulty::new(0).is_ok());
        assert!(Difficulty::new(5).is_ok());
        assert!(Difficulty::new(10).is_ok());
    }

    #[test]
    fn difficulty_rejects_out_of_range() {
        assert!(Difficulty::new(11).is_err());
        assert!(Difficulty::new(255).is_err());
    }

    #[test]
    fn user_prompt_rejects_too_long() {
        let long = "x".repeat(2001);
        assert!(UserPrompt::new(long).is_err());
    }

    #[test]
    fn user_prompt_accepts_max_length() {
        let exact = "x".repeat(2000);
        assert!(UserPrompt::new(exact).is_ok());
    }

    #[test]
    fn node_name_rejects_too_long() {
        let long = "x".repeat(201);
        assert!(NodeName::new(long).is_err());
    }

    #[test]
    fn serde_roundtrip_card_count() {
        let cc = CardCount::new(15).unwrap();
        let json = serde_json::to_string(&cc).unwrap();
        assert_eq!(json, "15");
        let parsed: CardCount = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cc);
    }

    #[test]
    fn serde_rejects_invalid_card_count() {
        let result: Result<CardCount, _> = serde_json::from_str("50");
        assert!(result.is_err());
    }

    #[test]
    fn learn_ahead_minutes_default() {
        let lam = LearnAheadMinutes::default();
        assert_eq!(lam.get(), 20);
    }
}
