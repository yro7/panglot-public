// Shared SQL fragments used by the db submodules.
// Bind-order comments must match placeholder counts in each constant.

// DECK_CLOSURE_CTE binds (in order):
//   1. user_id (closure anchor)
//   2. user_id (closure recursion)
//   3. user_id (review_counts)
pub(super) const DECK_CLOSURE_CTE: &str = r#"
WITH RECURSIVE deck_closure(ancestor_id, descendant_id) AS (
    SELECT id, id FROM decks WHERE user_id = ?
    UNION ALL
    SELECT dc.ancestor_id, d.id
    FROM deck_closure dc
    JOIN decks d ON d.parent_id = dc.descendant_id
    WHERE d.user_id = ?
),
review_counts AS (
    SELECT card_id, COUNT(*) as review_count
    FROM review_log WHERE user_id = ? GROUP BY card_id
)
"#;

// DECK_SUMMARY_SELECT binds (after CTE): due_cutoff ×3, reviews.user_id.
pub(super) const DECK_SUMMARY_SELECT: &str = r#"
SELECT
    d.id, d.parent_id, d.name, d.full_path, d.target_language,
    COUNT(c.id) as total_cards,
    SUM(CASE WHEN r.due_date <= ? AND r.interval_days = 0
              AND COALESCE(rc.review_count, 0) = 0 THEN 1 ELSE 0 END) as due_new_cards,
    SUM(CASE WHEN ((r.interval_days > 0 AND r.interval_days < 1)
                OR (r.interval_days = 0 AND COALESCE(rc.review_count, 0) > 0))
              AND r.due_date <= ? THEN 1 ELSE 0 END) as due_learning_cards,
    SUM(CASE WHEN r.interval_days >= 1 AND r.due_date <= ? THEN 1 ELSE 0 END) as due_review_cards
FROM decks d
LEFT JOIN deck_closure dc ON dc.ancestor_id = d.id
LEFT JOIN cards c ON c.deck_id = dc.descendant_id
LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
LEFT JOIN review_counts rc ON c.id = rc.card_id
"#;

// DECK_TREE_CTE binds: deck_id, user_id (anchor), user_id (recursion).
pub(super) const DECK_TREE_CTE: &str = r#"
WITH RECURSIVE deck_tree(id) AS (
    SELECT id FROM decks WHERE id = ? AND user_id = ?
    UNION ALL
    SELECT d.id FROM decks d JOIN deck_tree dt ON d.parent_id = dt.id WHERE d.user_id = ?
)
"#;

pub(super) const STUDY_CARD_PROJECTION: &str = r#"
c.id, c.deck_id,
COALESCE(c.skill_id, '') as skill_id,
COALESCE(c.skill_name, '') as skill_name,
COALESCE(c.template_name, '') as template_name,
c.front_html, c.back_html, c.metadata_json, c.audio_path
"#;

pub(super) const UPSERT_REVIEW_CACHE_SQL: &str = "INSERT INTO reviews (card_id, user_id, due_date, interval_days) VALUES (?, ?, ?, ?) \
     ON CONFLICT(card_id, user_id) DO UPDATE \
     SET due_date = excluded.due_date, interval_days = excluded.interval_days";
