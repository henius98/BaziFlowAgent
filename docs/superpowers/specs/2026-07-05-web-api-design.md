# Web API for Third-Party Access & Telegram API Key Management

## Purpose

Expose the existing BaziFlowAgent services as a RESTful Web API so third-party applications can consume Bazi analysis, daily fortune, and date selection features programmatically. Add a new Telegram `/apikey` command for users to generate Bearer tokens tied to their account.

## Architecture

The Web API runs on the **same axum server** (port 8080) that already serves static Bazi chart files. New routes are mounted under `/api/v1/...` alongside the existing `public/` fallback. The API reuses the existing `services/` layer (which is already bot-framework-agnostic) — no business logic duplication.

API keys are generated via a Telegram `/apikey` command. Each user gets **one active key**; generating a new key revokes the old one. Keys are stored as **SHA-256 hashes** in the database — the raw key is shown only once at generation time.

## Authentication

- **Method:** `Authorization: Bearer <api_key>` header
- **Key format:** `bfa_` prefix + 32 random hex chars (e.g., `bfa_a1b2c3d4e5f6...`)
- **Storage:** SHA-256 hash of the full key stored in `api_keys` table
- **Lookup:** On each request, hash the incoming Bearer token and look up the hash in the DB to resolve the `user_id`

## Database Changes

New table `api_keys`:

```sql
CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL UNIQUE,
    key_hash TEXT NOT NULL UNIQUE,       -- SHA-256 hex of the full API key
    key_prefix TEXT NOT NULL,            -- First 8 chars for display ("bfa_a1b2...")
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);
```

- `UNIQUE` on `user_id` enforces one-key-per-user (INSERT OR REPLACE to revoke old)
- `key_prefix` stored for display in `/apikey` status without exposing the full key

## API Endpoints

All endpoints require `Authorization: Bearer <api_key>` header. All return JSON.

### `POST /api/v1/profile`

Create or update the user's Bazi profile.

**Request body:**
```json
{
  "gender": 1,
  "birth_date": "1990-05-15",
  "birth_hour": 14,
  "birth_minute": 30,
  "location": "新加坡 (Singapore)"
}
```

- `gender`: `0` = female, `1` = male (required)
- `birth_date`: `YYYY-MM-DD` format (required)
- `birth_hour`: 0-23 (required)
- `birth_minute`: 0-59 (required)
- `location`: city name from the supported list (optional, defaults to Standard Time 120°E)

> **Note:** The `username` for `BaziDataParams` is resolved from the `users` table (stored when the user first interacted via Telegram). If no username exists, the string `"api_user"` is used as fallback.

**Response (200):**
```json
{
  "status": "ok",
  "chart_url": "https://example.com/bazi_12345.html",
  "bazi_analysis": "...(full LLM-generated destiny reading)...",
  "bazi_summary": "...(condensed summary)..."
}
```

When `?stream=true`, returns SSE with `Content-Type: text/event-stream`:
```
data: {"type": "chart_url", "content": "https://..."}

data: {"type": "analysis_chunk", "content": "partial text..."}

data: {"type": "analysis_chunk", "content": "more text..."}

data: {"type": "summary", "content": "condensed summary..."}

data: [DONE]
```

### `GET /api/v1/profile`

Get the user's current Bazi profile.

**Response (200):**
```json
{
  "status": "ok",
  "profile": {
    "gender": "男,乾造",
    "solar_date": "1990-05-15 14:30",
    "lunar_date": "...",
    "pillars": [...],
    "chart_url": "https://...",
    "bazi_analysis": "...",
    "bazi_summary": "...",
    "llm_model": "anthropic/claude-opus-4.8",
    "schedule": "08:00"
  }
}
```

**Response (404) if no profile:**
```json
{
  "status": "error",
  "message": "No Bazi profile found. Create one first via POST /api/v1/profile."
}
```

### `POST /api/v1/date-fortune`

Analyze fortune for a specific date.

**Request body:**
```json
{
  "date": "2026-07-06"
}
```

**Response (200):**
```json
{
  "status": "ok",
  "almanac": "...(formatted almanac data)...",
  "analysis": "...(full LLM-generated daily reading)..."
}
```

Supports `?stream=true` for SSE streaming.

### `POST /api/v1/pick-date`

Find auspicious dates for an activity within a date range.

**Request body:**
```json
{
  "start_date": "2026-07-10",
  "end_date": "2026-07-20",
  "activity": "搬家"
}
```

**Response (200):**
```json
{
  "status": "ok",
  "analysis": "...(full LLM-generated date selection reading)..."
}
```

Supports `?stream=true` for SSE streaming. Max 14-day range (enforced by existing logic).

### `PUT /api/v1/model`

Change the user's LLM model preference.

**Request body:**
```json
{
  "model": 0
}
```

- `model`: numeric ID matching `LlmModel` enum (0=Claude, 1=GPT, 2=Gemini)

**Response (200):**
```json
{
  "status": "ok",
  "model": "anthropic/claude-opus-4.8"
}
```

### `PUT /api/v1/schedule`

Set or clear daily fortune delivery schedule.

**Request body:**
```json
{
  "time": "08:00"
}
```

- `time`: `HH:MM` format, or `null` to disable

**Response (200):**
```json
{
  "status": "ok",
  "schedule": "08:00"
}
```

### `POST /api/v1/chat`

Send a follow-up message in the user's conversation context.

**Request body:**
```json
{
  "message": "我的事业运如何？"
}
```

**Response (200):**
```json
{
  "status": "ok",
  "reply": "...(LLM response)..."
}
```

Supports `?stream=true` for SSE streaming.

## Error Responses

All errors return consistent JSON:

```json
{
  "status": "error",
  "message": "Human-readable error description"
}
```

- `401 Unauthorized` — Missing or invalid API key
- `400 Bad Request` — Missing/invalid request fields
- `404 Not Found` — Resource not found (e.g., no profile)
- `500 Internal Server Error` — Upstream API or LLM failure

## Telegram `/apikey` Command

New command added to the `Command` enum:

```
/apikey — 🔑 API Key: Generate or view your API key for third-party access
```

**Behavior:**
1. If user has no key → generate a new one, show it with a warning that it won't be shown again
2. If user already has a key → show key prefix + creation date, with an inline keyboard button to regenerate (which revokes the old key)

**Telegram message format (new key):**
```
🔑 Your API Key (show only once):

bfa_a1b2c3d4e5f6789012345678901234

⚠️ Save this key now — it will NOT be shown again.

Usage: Authorization: Bearer bfa_a1b2c3d4...
Endpoint: https://your-domain.com/api/v1/
```

**Telegram message format (existing key):**
```
🔑 API Key Status:

Key: bfa_a1b2****
Created: 2026-07-05

[🔄 Regenerate Key]
```

## Module Structure

New files:

```
src/
├── api/
│   ├── mod.rs          — Module declarations + axum Router builder
│   ├── auth.rs         — Bearer token extraction + validation middleware
│   ├── handlers.rs     — Request handlers for all 7 endpoints
│   └── models.rs       — Request/Response serde structs
```

Modified files:

```
src/main.rs             — Mount API router on existing axum server
src/lib.rs              — Add `pub mod api;`
src/bot/commands.rs     — Add ApiKey variant to Command enum
src/bot/callbacks.rs    — Handle regenerate-key callback
src/repos/mod.rs        — Add api_keys CRUD functions
migrations/             — New migration for api_keys table
```

## Key Architectural Decisions

1. **`services/` stays untouched** — the API layer calls `services::bazi_service`, `services::almanac`, and `services::llm` directly, just like the Telegram bot handlers do. No business logic duplication.

2. **`api/handlers.rs` mirrors `bot/command_actions.rs`** — same orchestration pattern (fetch profile → call service → format response), but returns JSON instead of sending Telegram messages.

3. **SSE streaming** reuses the existing `tokio::sync::mpsc::Receiver<String>` from `LlmResponse::Stream`. The handler reads from the channel and writes SSE `data:` frames.

4. **Auth middleware** is an axum extractor that resolves `Bearer <token>` → `user_id` before handlers run. On failure, returns 401.

5. **No rate limiting** in the initial implementation. LLM cost acts as a natural throttle.

## Non-Goals (Out of Scope)

- API key scopes/permissions (all keys have full access to the owner's account)
- Multi-key support per user
- API key expiration
- Rate limiting
- Webhook/push notifications via API
- OpenAPI/Swagger documentation generation (can be added later)
