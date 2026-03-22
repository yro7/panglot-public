-- Extend usage_logs to support post-processing events (TTS, IPA) alongside LLM events.
ALTER TABLE usage_logs ADD COLUMN event_type TEXT NOT NULL DEFAULT 'llm';
ALTER TABLE usage_logs ADD COLUMN input_chars INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_usage_event_type ON usage_logs(user_id, event_type, created_at);
