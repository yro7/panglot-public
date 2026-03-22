CREATE TABLE IF NOT EXISTS usage_logs (
    id            INTEGER PRIMARY KEY,
    user_id       TEXT NOT NULL,
    request_id    TEXT NOT NULL,
    language      TEXT,
    endpoint      TEXT NOT NULL,
    provider      TEXT NOT NULL,
    model         TEXT NOT NULL,
    call_type     TEXT NOT NULL,
    tokens_in     INTEGER NOT NULL,
    tokens_out    INTEGER NOT NULL,
    latency_ms    INTEGER NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_user_created ON usage_logs(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_usage_user_model ON usage_logs(user_id, model, created_at);
