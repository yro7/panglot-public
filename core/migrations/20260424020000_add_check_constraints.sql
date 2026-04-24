-- Domain-integrity CHECK constraints.
--
-- Affected tables:
--   review_log.rating              ∈ [1, 4]   (1=Again, 2=Hard, 3=Good, 4=Easy)
--   review_log.reviewed_at         > 0
--   practice_log.rating            ∈ [1, 4]
--   practice_log.practiced_at      > 0
--   user_tree_customizations.action ∈ ('add','hide','edit')
--
-- SQLite does not support ALTER TABLE ADD CONSTRAINT; each table is rebuilt.
-- sqlx wraps this migration in a transaction, so no explicit BEGIN/COMMIT.

-- review_log ────────────────────────────────
CREATE TABLE review_log_v2 (
    id          INTEGER PRIMARY KEY,
    card_id     TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    rating      INTEGER NOT NULL CHECK(rating BETWEEN 1 AND 4),
    reviewed_at INTEGER NOT NULL CHECK(reviewed_at > 0),
    FOREIGN KEY(card_id) REFERENCES cards(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

INSERT INTO review_log_v2 (id, card_id, user_id, rating, reviewed_at)
SELECT id, card_id, user_id, rating, reviewed_at FROM review_log;

DROP TABLE review_log;
ALTER TABLE review_log_v2 RENAME TO review_log;

CREATE INDEX IF NOT EXISTS idx_review_log_card_id ON review_log(card_id);
CREATE INDEX IF NOT EXISTS idx_review_log_user_card_reviewed
    ON review_log(user_id, card_id, reviewed_at);
CREATE INDEX IF NOT EXISTS idx_review_log_user_rating
    ON review_log(user_id, rating, card_id);

-- practice_log ──────────────────────────────
CREATE TABLE practice_log_v2 (
    id           INTEGER PRIMARY KEY,
    card_id      TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    rating       INTEGER NOT NULL CHECK(rating BETWEEN 1 AND 4),
    practiced_at INTEGER NOT NULL CHECK(practiced_at > 0),
    FOREIGN KEY(card_id) REFERENCES cards(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

INSERT INTO practice_log_v2 (id, card_id, user_id, rating, practiced_at)
SELECT id, card_id, user_id, rating, practiced_at FROM practice_log;

DROP TABLE practice_log;
ALTER TABLE practice_log_v2 RENAME TO practice_log;

CREATE INDEX IF NOT EXISTS idx_practice_log_card_id ON practice_log(card_id);
CREATE INDEX IF NOT EXISTS idx_practice_log_user_card ON practice_log(user_id, card_id);

-- user_tree_customizations ──────────────────
CREATE TABLE user_tree_customizations_v3 (
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
);

INSERT INTO user_tree_customizations_v3
SELECT user_id, tree_definition_id, node_id, action, parent_id,
       node_name, node_instructions, prerequisites_json, sort_order, created_at
FROM user_tree_customizations;

DROP TABLE user_tree_customizations;
ALTER TABLE user_tree_customizations_v3 RENAME TO user_tree_customizations;

CREATE INDEX IF NOT EXISTS idx_user_tree_customizations_user_tree
    ON user_tree_customizations(user_id, tree_definition_id);
