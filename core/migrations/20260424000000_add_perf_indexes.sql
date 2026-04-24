-- Performance indexes for refactored queries (db.rs cleanup 2026-04-24)

-- Recursive CTEs deck_closure / deck_tree join on d.parent_id = <ancestor>.id.
-- Without this index each recursion level scans all of decks for the user.
CREATE INDEX IF NOT EXISTS idx_decks_parent_id
    ON decks(parent_id)
    WHERE parent_id IS NOT NULL;

-- rebuild_scheduling_cache issues:
--   SELECT card_id, rating, reviewed_at FROM review_log
--   WHERE user_id = ? ORDER BY card_id, reviewed_at
-- Extending the existing (user_id, card_id) index with reviewed_at lets the
-- ORDER BY be served index-only — no filesort on thousands of rows.
CREATE INDEX IF NOT EXISTS idx_review_log_user_card_reviewed
    ON review_log(user_id, card_id, reviewed_at);

-- The previous (user_id, card_id) index is a prefix of the new one; drop it
-- to avoid redundant write overhead on every review_log insert.
DROP INDEX IF EXISTS idx_review_log_user_card;
