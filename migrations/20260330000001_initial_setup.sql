-- Initial database setup with standardized datetime formats (until second)
-- Using YYYY-MM-DD HH:MM:SS format for all timestamps
-- ==============================================================================
-- 1. Users
-- ==============================================================================
CREATE TABLE IF NOT EXISTS users (
  user_id INTEGER PRIMARY KEY, -- Telegram i64 ID, NOT auto-generated
  username TEXT, -- Telegram @handle, nullable (not all users set one)
  gender INTEGER CHECK (gender IN (0, 1)), -- 0=female, 1=male; NULL until /new flow completes
  birth_datetime TEXT, -- ISO8601 "YYYY-MM-DD HH:MM:SS"; NULL until /new flow completes
  llm_model INTEGER REFERENCES llm_models (model_id) ON DELETE SET NULL, -- See llm_models for active mapping; NULL=fallback to default model (model_id=0)
  schedule TEXT, -- cron for user receive scheduled daily date fortune analysis report; if NULL then disable schedule for this user
  bazi_four_pillars BLOB, -- JSON stored via jsonb(); BLOB affinity matches binary format
  bazi_analysis TEXT, -- LLM-generated destiny text
  bazi_summary TEXT, -- main essential keypoint life for improve user date fortune analysis + save token
  rate_limit INTEGER, -- Per-user rate limit (requests per minute). NULL defaults to global config API_REQUESTS_PER_MINUTE
  last_active_at INTEGER NOT NULL DEFAULT (UNIXEPOCH())
);

-- Trigger to automatically update last_active_at on any change to the user row
CREATE TRIGGER IF NOT EXISTS trg_users_last_active_at AFTER
UPDATE ON users FOR EACH ROW WHEN NEW.last_active_at IS OLD.last_active_at -- Only auto-update if not explicitly set in the UPDATE query
BEGIN
UPDATE users
SET
  last_active_at = UNIXEPOCH()
WHERE
  user_id = NEW.user_id;

END;

-- Add an index on schedule to speed up get_all_scheduled_users background jobs
CREATE INDEX IF NOT EXISTS idx_users_schedule ON users (schedule);

-- View for user profiles with parsed JSON bazi four pillars
CREATE VIEW IF NOT EXISTS v_users AS
SELECT
  "user_id",
  "username",
  "gender",
  "birth_datetime",
  "llm_model",
  "schedule",
  JSON("bazi_four_pillars") AS "bazi_four_pillars",
  "bazi_analysis",
  "bazi_summary",
  "rate_limit",
  STRFTIME('%Y-%m-%d %H:%M:%S', "last_active_at", 'unixepoch', '+8 hours') AS "last_active_at"
FROM
  "users";

-- ==============================================================================
-- 2. Available LLM Models
-- ==============================================================================
-- Operator-managed availability settings for the LLM models known to this release.
-- users.llm_model keeps the selected model ID; unavailable selections fall back to default model (model_id=0).
CREATE TABLE IF NOT EXISTS llm_models (
  model_id INTEGER PRIMARY KEY AUTOINCREMENT,
  model_name TEXT NOT NULL UNIQUE,
  is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1))
);

-- Seed the models supported by the application as the single source of truth.
INSERT OR IGNORE INTO
  llm_models (model_id, model_name, is_enabled)
VALUES
  (0, 'gemini-3.1-flash-lite', 1),
  (1, 'gemini-3.8-flash', 1),
  (2, 'gemini-3.1-pro-preview', 1);

-- ==============================================================================
-- 3. API Keys
-- ==============================================================================
-- API key management for third-party Web API access
CREATE TABLE IF NOT EXISTS api_keys (
  id INTEGER PRIMARY KEY, -- generate_time_id()
  user_id INTEGER NOT NULL UNIQUE,
  key_hash TEXT NOT NULL UNIQUE,
  key_prefix TEXT NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users (user_id)
);

-- ==============================================================================
-- 4. LLM Logs
-- ==============================================================================
-- LLM call logging table to record every request and response
CREATE TABLE IF NOT EXISTS llm_logs (
  id INTEGER PRIMARY KEY, -- timestamp_millis * 1000 + random(0..999)
  user_id INTEGER NOT NULL,
  request_type TEXT,
  model_id INTEGER,
  reasoning_effort TEXT, -- Reasoning level requested per LLM call (e.g., "low", "medium", "high")
  request_body TEXT NOT NULL, -- Full LlmRequestParams serialized as JSON
  response_body TEXT NOT NULL, -- JSON response on success, error message on failure
  total_tokens INTEGER, -- Extracted from response usage (NULL on failure)
  duration_ms INTEGER NOT NULL, -- Wall-clock time of the API call
  is_success INTEGER NOT NULL DEFAULT 1 -- 0 = failed, 1 = success
);

-- Add an index on llm_logs user_id for faster lookups or user data purging
CREATE INDEX IF NOT EXISTS idx_llm_logs_user_id ON llm_logs (user_id);

-- Add an index on model_id for fast filtering
CREATE INDEX IF NOT EXISTS idx_llm_logs_model_id ON llm_logs (model_id);
