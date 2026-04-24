CREATE TABLE generation_batches (
    id                  TEXT PRIMARY KEY,
    user_id             TEXT NOT NULL,
    language_iso        TEXT NOT NULL,
    tree_definition_id  TEXT NOT NULL,
    node_id             TEXT NOT NULL,
    skill_id            TEXT NOT NULL,
    skill_name          TEXT NOT NULL,
    card_model_id       TEXT NOT NULL,
    default_deck_name   TEXT NOT NULL,
    materialized_deck_id TEXT,
    created_at          INTEGER NOT NULL,
    expires_at          INTEGER NOT NULL,

    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_generation_batches_user_id ON generation_batches(user_id);
CREATE INDEX idx_generation_batches_expires_at ON generation_batches(expires_at);

CREATE TABLE generation_batch_cards (
    id                   TEXT PRIMARY KEY,
    generation_batch_id  TEXT NOT NULL,
    template_name        TEXT NOT NULL,
    front_html           TEXT NOT NULL,
    back_html            TEXT NOT NULL,
    explanation_html     TEXT NOT NULL DEFAULT '',
    fields_json          TEXT NOT NULL DEFAULT '{}',
    metadata_json        TEXT NOT NULL DEFAULT '{}',
    audio_path           TEXT,
    created_at           INTEGER NOT NULL,

    FOREIGN KEY(generation_batch_id) REFERENCES generation_batches(id) ON DELETE CASCADE
);

CREATE INDEX idx_generation_batch_cards_batch_id ON generation_batch_cards(generation_batch_id);
