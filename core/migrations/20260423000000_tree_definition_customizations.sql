-- Tree customizations are now scoped to a tree definition rather than only a language.
-- Existing rows are migrated to the default tree definition for each language.

CREATE TABLE IF NOT EXISTS user_tree_customizations_v2 (
    user_id            TEXT NOT NULL,
    tree_definition_id TEXT NOT NULL,
    node_id            TEXT NOT NULL,
    action             TEXT NOT NULL,            -- 'add' | 'hide' | 'edit'
    parent_id          TEXT,                     -- required for 'add'
    node_name          TEXT,                     -- custom name (for 'add' or 'edit')
    node_instructions  TEXT,                     -- LLM instructions (for 'add' or 'edit')
    prerequisites_json TEXT,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,

    PRIMARY KEY (user_id, tree_definition_id, node_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

INSERT INTO user_tree_customizations_v2 (
    user_id,
    tree_definition_id,
    node_id,
    action,
    parent_id,
    node_name,
    node_instructions,
    prerequisites_json,
    sort_order,
    created_at
)
SELECT
    user_id,
    CASE language
        WHEN 'ara' THEN '390f554a-c379-48f9-b5cb-bdcd4a2367f7'
        WHEN 'cmn' THEN '01a02d30-b124-414f-97d6-4a172012e36a'
        WHEN 'dan' THEN 'c8ee894e-b3df-46dc-b17c-c467576a9fb1'
        WHEN 'jpn' THEN 'd52db4d5-750b-4a02-ac77-6e14d8d64326'
        WHEN 'kor' THEN 'd0211017-72f4-4615-8e0c-0ec1c69e038c'
        WHEN 'pol' THEN 'eb101138-bd6e-450a-abe0-a24c604d1a4e'
        WHEN 'rus' THEN '1f6cf552-984d-437a-ab20-cfe8307a4d50'
        WHEN 'tur' THEN '8707d223-e323-4a19-903e-0d13c65cf382'
        ELSE language
    END AS tree_definition_id,
    node_id,
    action,
    parent_id,
    node_name,
    node_instructions,
    prerequisites_json,
    sort_order,
    created_at
FROM user_tree_customizations;

DROP TABLE user_tree_customizations;

ALTER TABLE user_tree_customizations_v2 RENAME TO user_tree_customizations;

CREATE INDEX IF NOT EXISTS idx_user_tree_customizations_user_tree
    ON user_tree_customizations(user_id, tree_definition_id);
