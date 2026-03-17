-- Per-user skill tree customizations (overlay on top of base YAML tree)
--
-- Actions:
--   'add'  — user-created node (parent_id required)
--   'hide' — hide a base tree node from user's view
--   'edit' — override name and/or instructions of a base tree node

CREATE TABLE IF NOT EXISTS user_tree_customizations (
    user_id           TEXT NOT NULL,
    language          TEXT NOT NULL,            -- ISO 639-3
    node_id           TEXT NOT NULL,            -- node added or modified
    action            TEXT NOT NULL,            -- 'add' | 'hide' | 'edit'
    parent_id         TEXT,                     -- required for 'add'
    node_name         TEXT,                     -- custom name (for 'add' or 'edit')
    node_instructions TEXT,                     -- LLM instructions (for 'add' or 'edit')
    sort_order        INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,         -- Unix ms

    PRIMARY KEY (user_id, language, node_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_tree_customizations_user_lang
    ON user_tree_customizations(user_id, language);
