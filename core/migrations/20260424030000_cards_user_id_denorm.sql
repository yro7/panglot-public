-- Denormalize cards.user_id.
--
-- Every ownership check today joins cards → decks to filter by decks.user_id.
-- Adding cards.user_id directly eliminates that join on hot read paths
-- (verify_card_ownership, fetch_cards, etc.).
--
-- Invariant (enforced by Rust application layer, not by SQL):
--   cards.user_id == (SELECT user_id FROM decks WHERE id = cards.deck_id)
--
-- sqlx wraps this migration in a transaction — no explicit BEGIN/COMMIT.

CREATE TABLE cards_v2 (
    id            TEXT PRIMARY KEY,
    deck_id       TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    skill_id      TEXT,
    skill_name    TEXT NOT NULL DEFAULT '',
    template_name TEXT,
    front_html    TEXT NOT NULL,
    back_html     TEXT NOT NULL,
    fields_json   TEXT NOT NULL DEFAULT '{}',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    audio_path    TEXT,
    created_at    INTEGER NOT NULL,

    FOREIGN KEY(deck_id) REFERENCES decks(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

INSERT INTO cards_v2 (
    id, deck_id, user_id, skill_id, skill_name, template_name,
    front_html, back_html, fields_json, metadata_json, audio_path, created_at
)
SELECT
    c.id, c.deck_id, d.user_id, c.skill_id, c.skill_name, c.template_name,
    c.front_html, c.back_html, c.fields_json, c.metadata_json, c.audio_path, c.created_at
FROM cards c
JOIN decks d ON d.id = c.deck_id;

DROP TABLE cards;
ALTER TABLE cards_v2 RENAME TO cards;

CREATE INDEX IF NOT EXISTS idx_cards_deck_id ON cards(deck_id);
CREATE INDEX IF NOT EXISTS idx_cards_user_deck ON cards(user_id, deck_id);
