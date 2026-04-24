ALTER TABLE cards ADD COLUMN card_model_id TEXT NOT NULL DEFAULT '';

UPDATE cards
SET card_model_id = CASE template_name
    WHEN 'cloze_test' THEN 'ClozeTest'
    WHEN 'written_comprehension' THEN 'WrittenComprehension'
    WHEN 'oral_comprehension' THEN 'OralComprehension'
    ELSE COALESCE(template_name, '')
END
WHERE card_model_id = '';
