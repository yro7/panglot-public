-- Convert composite-PK tables to WITHOUT ROWID.
--
-- SQLite stores regular tables as a B-tree keyed by the implicit rowid, plus
-- a separate index for the declared PRIMARY KEY. WITHOUT ROWID collapses the
-- two: the table IS the B-tree keyed by the declared PK. For tables whose
-- PK is narrow and always queried via that PK (the two below), this:
--   * saves ~20-30% space per row (no rowid column, one B-tree instead of two)
--   * speeds PK lookups (one seek, not two)
--
-- Candidates applied: reviews, user_tree_customizations.
-- Skipped: users, decks, cards, draft_cards — single-column TEXT PKs benefit
--          little and the rowid is handy for diagnostics.

-- reviews ─────────────────────────────────
CREATE TABLE reviews_v2 (
    card_id       TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    due_date      INTEGER NOT NULL,
    interval_days REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (card_id, user_id),
    FOREIGN KEY(card_id) REFERENCES cards(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
) WITHOUT ROWID;

INSERT INTO reviews_v2 SELECT * FROM reviews;
DROP TABLE reviews;
ALTER TABLE reviews_v2 RENAME TO reviews;

CREATE INDEX IF NOT EXISTS idx_reviews_user_id ON reviews(user_id);
CREATE INDEX IF NOT EXISTS idx_reviews_user_due ON reviews(user_id, due_date);

-- user_tree_customizations ────────────────
CREATE TABLE user_tree_customizations_v4 (
    user_id            TEXT NOT NULL,
    tree_definition_id TEXT NOT NULL,
    node_id            TEXT NOT NULL,
    action             TEXT NOT NULL CHECK(action IN ('add','hide','edit')),
    parent_id          TEXT,
    node_name          TEXT,
    node_instructions  TEXT,
    prerequisites_json TEXT,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    PRIMARY KEY (user_id, tree_definition_id, node_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) WITHOUT ROWID;

INSERT INTO user_tree_customizations_v4 SELECT * FROM user_tree_customizations;
DROP TABLE user_tree_customizations;
ALTER TABLE user_tree_customizations_v4 RENAME TO user_tree_customizations;

CREATE INDEX IF NOT EXISTS idx_user_tree_customizations_user_tree
    ON user_tree_customizations(user_id, tree_definition_id);
