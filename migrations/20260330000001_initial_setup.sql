-- Initial database setup with standardized datetime formats (until second)
-- Using YYYY-MM-DD HH:MM:SS format for all timestamps

CREATE TABLE IF NOT EXISTS users (
    user_id        INTEGER PRIMARY KEY,                                   -- Telegram i64 ID, NOT auto-generated
    username       TEXT,                                                   -- Telegram @handle, nullable (not all users set one)
    gender         INTEGER CHECK (gender IN (0, 1)),                      -- 0=female, 1=male; NULL until /new flow completes
    birth_datetime TEXT,                                                   -- ISO8601 "YYYY-MM-DD HH:MM:SS"; NULL until /new flow completes
    bazi_four_pillars BLOB,                                                -- JSON stored via jsonb(); BLOB affinity matches binary format
    destiny_reading   TEXT     NOT NULL DEFAULT '',                        -- LLM-generated destiny text
    created_at        TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    last_active_at    TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);

-- Trigger to automatically update last_active_at on any change to the user row
CREATE TRIGGER IF NOT EXISTS trg_users_last_active_at
AFTER UPDATE ON users
FOR EACH ROW
WHEN NEW.last_active_at IS OLD.last_active_at -- Only auto-update if not explicitly set in the UPDATE query
BEGIN
    UPDATE users SET last_active_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
    WHERE user_id = NEW.user_id;
END;

CREATE TABLE IF NOT EXISTS requests (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL,
    request_type TEXT    NOT NULL,                                         -- e.g. "command", "message", "calendar_date", "new_bazi_reading"
    target_date  TEXT,                                                     -- ISO8601 date "YYYY-MM-DD", nullable
    text_content TEXT,
    llm_response TEXT,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requests_user_id ON requests(user_id, created_at);
