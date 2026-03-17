#[cfg(test)]
mod tests {
    use super::super::{Rating, SrsRegistry};

    #[test]
    fn rating_from_u8_valid() {
        assert_eq!(Rating::from_u8(1), Some(Rating::Again));
        assert_eq!(Rating::from_u8(2), Some(Rating::Hard));
        assert_eq!(Rating::from_u8(3), Some(Rating::Good));
        assert_eq!(Rating::from_u8(4), Some(Rating::Easy));
    }

    #[test]
    fn rating_from_u8_invalid() {
        assert_eq!(Rating::from_u8(0), None);
        assert_eq!(Rating::from_u8(5), None);
        assert_eq!(Rating::from_u8(255), None);
    }

    #[test]
    fn rating_from_str_lossy_valid() {
        assert_eq!(Rating::from_str_lossy("Again"), Rating::Again);
        assert_eq!(Rating::from_str_lossy("HARD"), Rating::Hard);
        assert_eq!(Rating::from_str_lossy("good"), Rating::Good);
        assert_eq!(Rating::from_str_lossy("Easy"), Rating::Easy);
        assert_eq!(Rating::from_str_lossy("3"), Rating::Good);
    }

    #[test]
    fn rating_from_str_lossy_invalid_defaults_to_again() {
        assert_eq!(Rating::from_str_lossy("banana"), Rating::Again);
        assert_eq!(Rating::from_str_lossy(""), Rating::Again);
        assert_eq!(Rating::from_str_lossy("99"), Rating::Again);
    }

    #[test]
    fn registry_get_sm2() {
        let reg = SrsRegistry::new();
        let algo = reg.get("sm2").unwrap();
        assert_eq!(algo.id(), "sm2");
        assert_eq!(algo.name(), "SM-2");
    }

    #[test]
    fn registry_get_leitner() {
        let reg = SrsRegistry::new();
        let algo = reg.get("leitner").unwrap();
        assert_eq!(algo.id(), "leitner");
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let reg = SrsRegistry::new();
        assert!(reg.get("unknown_algo").is_none());
    }

    #[test]
    fn registry_default_is_sm2() {
        let reg = SrsRegistry::new();
        assert_eq!(reg.default().id(), "sm2");
    }

    #[test]
    fn registry_list_contains_all() {
        let reg = SrsRegistry::new();
        let list = reg.list();
        assert_eq!(list.len(), 5);
        let ids: Vec<&str> = list.iter().map(|&(id, _)| id).collect();
        assert!(ids.contains(&"sm2"));
        assert!(ids.contains(&"leitner"));
        assert!(ids.contains(&"fsrs-4.5"));
        assert!(ids.contains(&"fsrs-5"));
        assert!(ids.contains(&"fsrs-6"));
    }

    #[test]
    fn preview_choices_empty_history() {
        let reg = SrsRegistry::new();
        let algo = reg.default();
        let now = 1_700_000_000_000i64;
        let choices = algo.preview_choices(&[], now);

        // All 4 should produce valid outputs
        assert!(choices.again.due_date >= now);
        assert!(choices.hard.due_date > now);
        assert!(choices.good.due_date > now);
        assert!(choices.easy.due_date > now);

        // Ordering: again <= hard <= good <= easy
        assert!(choices.again.due_date <= choices.hard.due_date);
        assert!(choices.hard.due_date <= choices.good.due_date);
        assert!(choices.good.due_date <= choices.easy.due_date);
    }
}
