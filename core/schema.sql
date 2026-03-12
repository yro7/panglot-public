-- Panglot Database Schema
-- SQLite — source of truth for the local study DB
-- This file is documentation only. The actual schema is applied by db.rs::create_schema().
--
-- Relations:
--
--   users 1──┬──< decks 1──< cards 1──< reviews
--            │                    │       │
--            │                    └──< review_log
--            ├──< draft_cards        users ┘
--            └──< lexicon
--
-- Architecture:
--   review_log = append-only, algorithm-agnostic (raw facts: rating + timestamp)
--   reviews    = materialized scheduling cache, recalculable by any SRS algorithm
--   Switching algorithm = rebuild_scheduling_cache() replays review_log history

PRAGMA foreign_keys = ON;

-- ══════════════════════════════════════════
--  USERS
-- ══════════════════════════════════════════
CREATE TABLE IF NOT EXISTS users (
    id            TEXT PRIMARY KEY,          -- UUID from Supabase JWT `sub`, or "default-user" in solo mode
    display_name  TEXT NOT NULL,             -- cosmetic, non-unique (email prefix, GitHub username, or "user")
    email         TEXT,                      -- NULL if OAuth without email or solo mode
    settings      TEXT NOT NULL DEFAULT '{}',
    created_at    INTEGER NOT NULL DEFAULT 0 -- Unix ms
);

-- No unique index on email: two providers can yield the same email,
-- or no email at all (GitHub, phone, anonymous sign-in)

-- ══════════════════════════════════════════
--  DECKS  (hierarchical via parent_id)
-- ══════════════════════════════════════════
--  full_path uses "::" separator, e.g. "Polish::Grammar::Genitive"
CREATE TABLE IF NOT EXISTS decks (
    id              TEXT PRIMARY KEY,        -- UUID v4
    user_id         TEXT NOT NULL,
    parent_id       TEXT,                    -- NULL = root deck
    name            TEXT NOT NULL,           -- leaf name ("Genitive")
    full_path       TEXT NOT NULL,           -- "Polish::Grammar::Genitive"
    target_language TEXT,                    -- ISO 639-3 code
    created_at      INTEGER NOT NULL,        -- Unix ms

    UNIQUE(full_path, user_id),
    FOREIGN KEY(user_id)   REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(parent_id) REFERENCES decks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_decks_user_id ON decks(user_id);

-- ══════════════════════════════════════════
--  CARDS
-- ══════════════════════════════════════════
CREATE TABLE IF NOT EXISTS cards (
    id            TEXT PRIMARY KEY,           -- UUID v4
    deck_id       TEXT NOT NULL,
    skill_id      TEXT,                       -- skill tree node id
    template_name TEXT,                       -- e.g. "default"
    front_html    TEXT NOT NULL,
    back_html     TEXT NOT NULL,
    fields_json   TEXT NOT NULL DEFAULT '{}', -- raw field k/v
    metadata_json TEXT NOT NULL DEFAULT '{}', -- pedagogical_explanation, ipa, etc.
    audio_path    TEXT,                       -- relative path to audio file
    created_at    INTEGER NOT NULL,           -- Unix ms

    FOREIGN KEY(deck_id) REFERENCES decks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cards_deck_id ON cards(deck_id);

-- ══════════════════════════════════════════
--  REVIEWS  (scheduling cache, algorithm-agnostic)
-- ══════════════════════════════════════════
--  This is a materialized cache derived from review_log by the active SRS algorithm.
--  Recalculable at any time via rebuild_scheduling_cache().
--  reps/lapses are derived from review_log when needed (COUNT queries).
CREATE TABLE IF NOT EXISTS reviews (
    card_id       TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    due_date      INTEGER NOT NULL,            -- Unix ms — when this card is next due
    interval_days REAL NOT NULL DEFAULT 0,     -- current interval in days (0.0 = new)

    PRIMARY KEY (card_id, user_id),
    FOREIGN KEY(card_id) REFERENCES cards(id)  ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id)  ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reviews_user_id ON reviews(user_id);

-- ══════════════════════════════════════════
--  LEXICON  (vocabulary tracker)
-- ══════════════════════════════════════════
CREATE TABLE IF NOT EXISTS lexicon (
    id               TEXT PRIMARY KEY,        -- UUID v4
    user_id          TEXT NOT NULL,
    language         TEXT NOT NULL,            -- ISO 639-3
    lemma            TEXT NOT NULL,            -- dictionary form
    pos              TEXT NOT NULL,            -- part of speech
    morphology_json  TEXT NOT NULL DEFAULT '{}',
    status           TEXT NOT NULL,            -- 'Seen' | 'Mastered'
    mastered_at      INTEGER,                 -- Unix ms, NULL if not mastered

    UNIQUE(user_id, language, lemma, pos),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_lexicon_user_id ON lexicon(user_id);

-- ══════════════════════════════════════════
--  REVIEW_LOG  (append-only, algorithm-agnostic history)
-- ══════════════════════════════════════════
--  Source of truth for SRS. Only raw facts: which rating at which time.
--  The SRS algorithm replays this history to compute scheduling.
CREATE TABLE IF NOT EXISTS review_log (
    id          INTEGER PRIMARY KEY,  -- no AUTOINCREMENT: append-only, no rowid reuse risk
    card_id     TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    rating      INTEGER NOT NULL,    -- 1=Again, 2=Hard, 3=Good, 4=Easy
    reviewed_at INTEGER NOT NULL,    -- Unix ms

    FOREIGN KEY(card_id) REFERENCES cards(id)  ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id)  ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_review_log_card_id ON review_log(card_id);
CREATE INDEX IF NOT EXISTS idx_review_log_user_card ON review_log(user_id, card_id);

-- ══════════════════════════════════════════
--  DRAFT_CARDS  (temporary generated cards, pre-save)
-- ══════════════════════════════════════════
--  Cards land here after LLM generation, before the user saves them to a deck.
--  Cleared on explicit user action or when saved to a real deck.
CREATE TABLE IF NOT EXISTS draft_cards (
    id              TEXT PRIMARY KEY,        -- UUID v4 (= card_id from generation)
    user_id         TEXT NOT NULL,
    skill_id        TEXT NOT NULL DEFAULT '',    -- empty if free generation (no skill tree)
    skill_name      TEXT NOT NULL DEFAULT '',    -- empty if free generation
    template_name   TEXT NOT NULL,
    fields_json     TEXT NOT NULL DEFAULT '{}',
    explanation     TEXT NOT NULL DEFAULT '',
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL,        -- Unix ms

    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_draft_cards_user_id ON draft_cards(user_id);
CREATE INDEX IF NOT EXISTS idx_draft_cards_created_at ON draft_cards(created_at);
