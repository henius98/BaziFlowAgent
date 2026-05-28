-- LLM call logging table to record every request and response
CREATE TABLE IF NOT EXISTS llm_logs (
    id INTEGER PRIMARY KEY, -- timestamp_millis, matches existing pattern
    model TEXT NOT NULL,
    request_body TEXT NOT NULL, -- Full LlmRequestParams serialized as JSON
    response_body TEXT NOT NULL, -- JSON response on success, error message on failure
    total_tokens INTEGER, -- Extracted from response usage (NULL on failure)
    duration_ms INTEGER NOT NULL, -- Wall-clock time of the API call
    is_success INTEGER NOT NULL DEFAULT 1 -- 0 = failed, 1 = success
);