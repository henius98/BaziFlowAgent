-- Add an index on schedule to speed up get_all_scheduled_users background jobs
CREATE INDEX IF NOT EXISTS idx_users_schedule ON users (schedule);

-- Add an index on last_active_at to speed up context and inactive user cleanups
CREATE INDEX IF NOT EXISTS idx_users_last_active_at ON users (last_active_at);

-- Add an index on llm_logs user_id for faster lookups or user data purging
CREATE INDEX IF NOT EXISTS idx_llm_logs_user_id ON llm_logs (user_id);
