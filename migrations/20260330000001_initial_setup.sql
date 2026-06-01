-- Initial database setup with standardized datetime formats (until second)
-- Using YYYY-MM-DD HH:MM:SS format for all timestamps
CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY, -- Telegram i64 ID, NOT auto-generated
    username TEXT, -- Telegram @handle, nullable (not all users set one)
    gender INTEGER CHECK (gender IN (0, 1)), -- 0=female, 1=male; NULL until /new flow completes
    birth_datetime TEXT, -- ISO8601 "YYYY-MM-DD HH:MM:SS"; NULL until /new flow completes
    bazi_four_pillars BLOB, -- JSON stored via jsonb(); BLOB affinity matches binary format
    bazi_analysis TEXT NOT NULL DEFAULT '', -- LLM-generated destiny text
    llm_model INTEGER DEFAULT NULL, -- 0=gpt-4o, 1=gemini-3.5-pro, 2=claude-3-5-sonnet, 3=deepseek-chat, 4=deepseek-reasoner; NULL=fallback to .env LLM_MODEL_NAME
    created_at TEXT NOT NULL DEFAULT (strftime ('%Y-%m-%d %H:%M:%S', 'now')),
    last_active_at TEXT NOT NULL DEFAULT (strftime ('%Y-%m-%d %H:%M:%S', 'now'))
);

-- Trigger to automatically update last_active_at on any change to the user row
CREATE TRIGGER IF NOT EXISTS trg_users_last_active_at AFTER
UPDATE ON users FOR EACH ROW WHEN NEW.last_active_at IS OLD.last_active_at -- Only auto-update if not explicitly set in the UPDATE query
BEGIN
UPDATE users
SET
    last_active_at = strftime ('%Y-%m-%d %H:%M:%S', 'now')
WHERE
    user_id = NEW.user_id;

END;

-- LLM call logging table to record every request and response
CREATE TABLE IF NOT EXISTS llm_logs (
    id INTEGER PRIMARY KEY, -- timestamp_millis, matches existing pattern
    user_id INTEGER NOT NULL,
    request_type TEXT,
    model TEXT NOT NULL,
    request_body TEXT NOT NULL, -- Full LlmRequestParams serialized as JSON
    response_body TEXT NOT NULL, -- JSON response on success, error message on failure
    total_tokens INTEGER, -- Extracted from response usage (NULL on failure)
    duration_ms INTEGER NOT NULL, -- Wall-clock time of the API call
    is_success INTEGER NOT NULL DEFAULT 1 -- 0 = failed, 1 = success
);