-- Performance indexes identified by codebase audit (2026-03-22)

-- Study session performance: get_due_cards_for_deck() filters on (user_id, due_date)
CREATE INDEX IF NOT EXISTS idx_reviews_user_due
    ON reviews(user_id, due_date);

-- Lapse counting: fetch_cards() and fetch_decks() filter on (user_id, rating)
CREATE INDEX IF NOT EXISTS idx_review_log_user_rating
    ON review_log(user_id, rating, card_id);

-- Per-language vocabulary: lexicon queries filter on (user_id, language)
CREATE INDEX IF NOT EXISTS idx_lexicon_user_lang
    ON lexicon(user_id, language);
