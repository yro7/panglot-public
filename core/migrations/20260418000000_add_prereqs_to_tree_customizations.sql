-- Adds JSON-encoded prerequisites list to per-user skill tree customizations.
--
-- Semantics (enforced by the overlay logic, not by SQL):
--   NULL       → for 'edit': leave base prereqs untouched. For 'add': empty list.
--   '[]'       → for 'edit': clear prereqs. For 'add': empty list.
--   '["a","b"]' → replace (edit) or initialize (add) prereqs with those IDs.

ALTER TABLE user_tree_customizations
    ADD COLUMN prerequisites_json TEXT;
