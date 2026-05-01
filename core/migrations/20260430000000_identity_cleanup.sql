-- Identity & link cleanup.
--
-- 1. Promote ephemeral `generation_batches` to permanent `generations` log.
--    `generation_batch_cards` becomes `generation_cards` (still TTL'd).
--    Drop `skill_id` / `skill_name` columns on the log (engine writes
--    source node info into card metadata_json instead).
-- 2. Add `decks.generation_id` FK to the log; drop `decks.full_path`
--    (derive on read via recursive CTE on parent_id chain).
-- 3. Drop `cards.skill_id` and `cards.skill_name` (lookups go through
--    deck.generation -> tree_node_id, display data lives in metadata_json).
-- 4. Migrate `draft_cards` from `skill_id`/`skill_name` to `generation_id`.
--
-- sqlx wraps the migration in a transaction. SQLite's default
-- foreign_keys pragma is OFF for migrations, so table swaps are safe.

-- ════════════════════════════════════════════════════════════════════
-- 1. NEW: generations + generation_cards
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE generations (
    id                       TEXT PRIMARY KEY,
    user_id                  TEXT NOT NULL,
    language_iso             TEXT NOT NULL,
    tree_definition_id       TEXT,
    tree_node_id             TEXT,
    concept_key              TEXT,
    card_model_id            TEXT NOT NULL,
    card_count               INTEGER NOT NULL DEFAULT 0,
    difficulty               INTEGER NOT NULL DEFAULT 0,
    user_prompt              TEXT,
    default_deck_name        TEXT NOT NULL,
    materialized_deck_id     TEXT,
    created_at               INTEGER NOT NULL,

    FOREIGN KEY(user_id)              REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(materialized_deck_id) REFERENCES decks(id) ON DELETE SET NULL
);

CREATE INDEX idx_generations_user_id            ON generations(user_id);
CREATE INDEX idx_generations_user_tree          ON generations(user_id, tree_definition_id);
CREATE INDEX idx_generations_materialized_deck  ON generations(materialized_deck_id)
    WHERE materialized_deck_id IS NOT NULL;

CREATE TABLE generation_cards (
    id               TEXT PRIMARY KEY,
    generation_id    TEXT NOT NULL,
    template_name    TEXT NOT NULL,
    front_html       TEXT NOT NULL,
    back_html        TEXT NOT NULL,
    explanation_html TEXT NOT NULL DEFAULT '',
    fields_json      TEXT NOT NULL DEFAULT '{}',
    metadata_json    TEXT NOT NULL DEFAULT '{}',
    audio_path       TEXT,
    created_at       INTEGER NOT NULL,
    expires_at       INTEGER NOT NULL,

    FOREIGN KEY(generation_id) REFERENCES generations(id) ON DELETE CASCADE
);

CREATE INDEX idx_generation_cards_generation_id ON generation_cards(generation_id);
CREATE INDEX idx_generation_cards_expires_at    ON generation_cards(expires_at);

-- ════════════════════════════════════════════════════════════════════
-- 2. BACKFILL generations from generation_batches
--    Legacy rows: card_count and difficulty default to 0; concept_key NULL.
-- ════════════════════════════════════════════════════════════════════

INSERT INTO generations (
    id, user_id, language_iso, tree_definition_id, tree_node_id,
    concept_key, card_model_id, card_count, difficulty, user_prompt,
    default_deck_name, materialized_deck_id, created_at
)
SELECT
    id, user_id, language_iso,
    NULLIF(tree_definition_id, ''), NULLIF(node_id, ''),
    NULL,
    card_model_id, 0, 0, NULL,
    default_deck_name, materialized_deck_id, created_at
FROM generation_batches;

INSERT INTO generation_cards (
    id, generation_id, template_name, front_html, back_html,
    explanation_html, fields_json, metadata_json, audio_path,
    created_at, expires_at
)
SELECT
    bc.id, bc.generation_batch_id, bc.template_name, bc.front_html, bc.back_html,
    bc.explanation_html, bc.fields_json, bc.metadata_json, bc.audio_path,
    bc.created_at, b.expires_at
FROM generation_batch_cards bc
JOIN generation_batches b ON b.id = bc.generation_batch_id;

DROP INDEX IF EXISTS idx_generation_batch_cards_batch_id;
DROP INDEX IF EXISTS idx_generation_batches_user_id;
DROP INDEX IF EXISTS idx_generation_batches_expires_at;
DROP TABLE generation_batch_cards;
DROP TABLE generation_batches;

-- ════════════════════════════════════════════════════════════════════
-- 3. REBUILD decks: drop full_path, add generation_id, new uniqueness
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE decks_v3 (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL,
    parent_id       TEXT,
    name            TEXT NOT NULL,
    target_language TEXT NOT NULL DEFAULT '',
    generation_id   TEXT,
    created_at      INTEGER NOT NULL,

    FOREIGN KEY(user_id)       REFERENCES users(id)        ON DELETE CASCADE,
    FOREIGN KEY(parent_id)     REFERENCES decks(id)        ON DELETE CASCADE,
    FOREIGN KEY(generation_id) REFERENCES generations(id)  ON DELETE SET NULL
);

INSERT INTO decks_v3 (id, user_id, parent_id, name, target_language, generation_id, created_at)
SELECT
    d.id, d.user_id, d.parent_id, d.name, d.target_language,
    (SELECT g.id FROM generations g WHERE g.materialized_deck_id = d.id LIMIT 1),
    d.created_at
FROM decks d;

DROP TABLE decks;
ALTER TABLE decks_v3 RENAME TO decks;

CREATE INDEX idx_decks_user_id   ON decks(user_id);
CREATE INDEX idx_decks_parent_id ON decks(parent_id) WHERE parent_id IS NOT NULL;
CREATE UNIQUE INDEX idx_decks_unique_sibling
    ON decks(user_id, IFNULL(parent_id, ''), name);

-- ════════════════════════════════════════════════════════════════════
-- 4. REBUILD cards: drop skill_id, skill_name. Keep card_model_id.
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE cards_v3 (
    id            TEXT PRIMARY KEY,
    deck_id       TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    card_model_id TEXT NOT NULL DEFAULT '',
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

INSERT INTO cards_v3 (
    id, deck_id, user_id, card_model_id, template_name,
    front_html, back_html, fields_json, metadata_json, audio_path, created_at
)
SELECT
    id, deck_id, user_id, card_model_id, template_name,
    front_html, back_html, fields_json, metadata_json, audio_path, created_at
FROM cards;

DROP TABLE cards;
ALTER TABLE cards_v3 RENAME TO cards;

CREATE INDEX idx_cards_deck_id    ON cards(deck_id);
CREATE INDEX idx_cards_user_deck  ON cards(user_id, deck_id);

-- ════════════════════════════════════════════════════════════════════
-- 5. REBUILD draft_cards: replace skill_id/skill_name with generation_id
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE draft_cards_v2 (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL,
    generation_id   TEXT,
    template_name   TEXT NOT NULL,
    fields_json     TEXT NOT NULL DEFAULT '{}',
    explanation     TEXT NOT NULL DEFAULT '',
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL,

    FOREIGN KEY(user_id)       REFERENCES users(id)       ON DELETE CASCADE,
    FOREIGN KEY(generation_id) REFERENCES generations(id) ON DELETE SET NULL
);

INSERT INTO draft_cards_v2 (
    id, user_id, generation_id, template_name,
    fields_json, explanation, metadata_json, created_at
)
SELECT
    id, user_id, NULL, template_name,
    fields_json, explanation, metadata_json, created_at
FROM draft_cards;

DROP TABLE draft_cards;
ALTER TABLE draft_cards_v2 RENAME TO draft_cards;

CREATE INDEX idx_draft_cards_user_id     ON draft_cards(user_id);
CREATE INDEX idx_draft_cards_created_at  ON draft_cards(created_at);
CREATE INDEX idx_draft_cards_generation  ON draft_cards(generation_id)
    WHERE generation_id IS NOT NULL;
