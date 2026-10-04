-- API key management for third-party Web API access
-- 'localtime' modifier converts UTC 'now' using the process TZ (set from APP_TIMEZONE in .env)
CREATE TABLE IF NOT EXISTS api_keys (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL UNIQUE,
  key_hash TEXT NOT NULL UNIQUE,
  key_prefix TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now', 'localtime')),
  FOREIGN KEY (user_id) REFERENCES users (user_id)
);
