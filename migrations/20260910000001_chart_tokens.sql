-- Unguessable, revocable capability links for locally hosted personal charts.
ALTER TABLE users ADD COLUMN chart_token TEXT;
CREATE UNIQUE INDEX idx_users_chart_token ON users(chart_token) WHERE chart_token IS NOT NULL;
