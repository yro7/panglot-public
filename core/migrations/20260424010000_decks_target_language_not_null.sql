-- Tighten decks.target_language: TEXT NULL → TEXT NOT NULL DEFAULT ''.
-- Removes COALESCE(d.target_language, '') noise from every deck query in db.rs.
--
-- SQLite can't ALTER COLUMN, so we rebuild the table. sqlx wraps each
-- migration file in a transaction, so no explicit BEGIN/COMMIT here.

CREATE TABLE decks_v2 (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL,
    parent_id       TEXT,
    name            TEXT NOT NULL,
    full_path       TEXT NOT NULL,
    target_language TEXT NOT NULL DEFAULT '',
    created_at      INTEGER NOT NULL,

    UNIQUE(full_path, user_id),
    FOREIGN KEY(user_id)   REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(parent_id) REFERENCES decks(id) ON DELETE CASCADE
);

INSERT INTO decks_v2 (id, user_id, parent_id, name, full_path, target_language, created_at)
SELECT id, user_id, parent_id, name, full_path,
       COALESCE(target_language, ''), created_at
FROM decks;

DROP TABLE decks;
ALTER TABLE decks_v2 RENAME TO decks;

-- Re-create indexes.
CREATE INDEX IF NOT EXISTS idx_decks_user_id ON decks(user_id);
CREATE INDEX IF NOT EXISTS idx_decks_parent_id
    ON decks(parent_id)
    WHERE parent_id IS NOT NULL;
