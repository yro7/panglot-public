ALTER TABLE cards ADD COLUMN skill_name TEXT NOT NULL DEFAULT '';

UPDATE cards
SET skill_name = COALESCE(skill_id, '')
WHERE skill_name = '';

CREATE TABLE IF NOT EXISTS practice_log (
    id           INTEGER PRIMARY KEY,
    card_id      TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    rating       INTEGER NOT NULL,
    practiced_at INTEGER NOT NULL,

    FOREIGN KEY(card_id) REFERENCES cards(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_practice_log_card_id ON practice_log(card_id);
CREATE INDEX IF NOT EXISTS idx_practice_log_user_card ON practice_log(user_id, card_id);
