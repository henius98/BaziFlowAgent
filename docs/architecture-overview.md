# BaziFlowAgent Architecture & Source Code Overview

Detailed reference document disclosing codebase layout, component maps, and quick navigation routes.

---

## 🛠 Tech Stack Overview

| Layer          | Library                                 | Purpose                                       |
| -------------- | --------------------------------------- | --------------------------------------------- |
| Bot Framework  | `teloxide` (macros enabled)             | Telegram Bot API & routing via `dptree`       |
| Async Runtime  | `tokio` (full features)                 | Multi-threaded async reactor                  |
| HTTP Client    | `reqwest`, `async-openai`               | External APIs & OpenAI-compatible LLM calls   |
| Database       | `sqlx` (SQLite, `runtime-tokio-rustls`) | Persist users, sessions, and requests         |
| Concurrency    | `dashmap`                               | Lock-free in-memory user session state        |
| Scheduling     | `tokio-cron-scheduler`                  | Daily almanac pulls & session GC              |
| Time           | `chrono`, `chrono-tz`                   | Timezone-aware date handling (SGT/UTC+8)      |
| Logging        | `tracing`, `tracing-subscriber`         | Structured async-safe logging                 |
| Serialization  | `serde`, `serde_json`                   | JSON parsing for API payloads                 |
| Web Server     | `axum`, `tower-http`, `tungstenite`     | REST API, WebSocket streams, and static files |
| Object Storage | `rust-s3`                               | Cloudflare R2 integration for HTML charts     |

---

## 📂 Source Code Map (`src/`)

### Entry Point & Infrastructure

- **`lib.rs`**: Library root declaring all public modules for external access or testing.
- **`main.rs`**: Loads `AppConfig`, initializes SQLite/D1 pool, constructs `AppState`, registers Telegram handlers via `dptree`, starts scheduler, and binds Axum server.
- **`logger.rs`**: Dual console + rotating file structured logging with retention cleanup.
- **`scheduler.rs`**: Cron jobs: dynamic daily fortune delivery, session context expiration, and log retention.
- **`utils.rs`**: Shared utilities (`split_message`, `get_formatted_bazi_four_pillars`, `parse_user_bazi`).

### `bot/` — Telegram Bot Handlers & UI

- **`mod.rs`**: Module declarations and `Command` re-export.
- **`commands.rs`**: Command definitions (`/new` [alias `/start`], `/date`, `/pick`, `/profile`, `/model`, `/schedule`, `/apikey`) and dispatch.
- **`callbacks.rs`**: Inline keyboard callback dispatcher across 9 namespaces.
- **`messages.rs`**: Free-text conversation message handler.
- **`command_actions.rs`**: Telegram UI orchestration for birthdate input and user profile display.
- **`helpers.rs`**: Telegram formatting and streaming token sender (`stream_to_telegram`).
- **`keyboards.rs`**: Inline calendar, picker, and setting keyboard builders.

### `config/` — Configuration

- **`mod.rs`**: `AppConfig` struct validating environment variables from `.env`.

### `models/` — Domain Types & Runtime

- **`mod.rs`**: Re-exports.
- **`common.rs`**: Common data types (`LlmModel`, `LlmStream` transferring `bytes::Bytes`).
- **`error.rs`**: `AppError`, `AppResult<T>`, and `LogErrorExt`.
- **`state.rs`**: `AppState` struct holding decoupled repository traits and `UserContext` (`VecDeque<ChatTurn>`).
- **`runtime.rs`**: Unified `AdmissionController`, `AdmissionPermit`, and `RateLimiter`.
- **`processing_guard.rs`**: RAII `ProcessingGuard` for user single-flight locking.
- **`events.rs`**: In-memory pub-sub event bus.

### `repos/` — Storage & Database Layer

- **`mod.rs`**: SQLite initialization and helper queries.
- **`traits.rs`**: Dyn-compatible contracts (`UserRepository`, `LlmLogRepository`, `ChatCacheRepository`, `BlobStore`).
- **`blob_store.rs`**: Concrete blob store implementations (`LocalFsBlobStore`, `R2BlobStore`).
- **`chat_history_cache.rs`**: Write-behind persistent chat turn cache with WAL sync.
- **`d1.rs`**: Cloudflare D1 REST client implementation.

### `api/` — Web API Layer

- **`mod.rs`**: Router setup and route registration.
- **`admission.rs`**: HTTP admission middleware adapting `AdmissionController` to HTTP 429/503 responses.
- **`auth.rs`**: Bearer token authentication middleware.
- **`handlers.rs`**: REST endpoints and WebSocket fortune streams.
- **`models.rs`**: Request and response DTO schemas.
- **`gateway.rs`**: Protocol gateway utilities.

### `services/` — Domain Business Logic

- **`mod.rs`**: Module declarations and re-exports.
- **`almanac.rs`**: External Almanac data fetch, schema pruning, and Kong Wang calculation.
- **`bazi_service.rs`**: Core Bazi chart generation, solar time calculation, R2 chart upload, and prompt construction.
- **`conversation.rs`**: Multi-turn chat session orchestration, zero-copy turn serialization, and prompt assembly.
- **`llm.rs`**: OpenAI-compatible LLM client streaming tokens as `bytes::Bytes`.
- **`solar_time.rs`**: True Solar Time calculation from longitude and equation of time.
- **`paipan/`**: Astrological chart calculation submodule (`client.rs`, `models.rs`, `formatter.rs`, `bazi_utils.rs`).

---

## ⚡ Quick Feature Reference

| Task                  | Where to edit                                             |
| --------------------- | --------------------------------------------------------- |
| Add a bot command     | `src/bot/commands.rs` `Command` enum + `handle_command`   |
| Add callback handling | `src/bot/callbacks.rs` callback dispatch namespace        |
| Modify shared state   | `src/models/state.rs` `AppState` struct                   |
| Add storage method    | `src/repos/traits.rs` repository traits + implementations |
| Modify admission/rate | `src/models/runtime.rs` `AdmissionController`             |
| Change API parsing    | `src/services/almanac.rs` `KEEP_SCHEMA` / `KEY_MAP`       |
| Tweak LLM parameters  | `src/services/llm.rs` `LlmRequestParams`                  |
| Add a scheduled job   | `src/scheduler.rs` — new `Job::new_async`                 |
| Change configuration  | `src/config/mod.rs` `AppConfig` + `.env`                  |
| Add keyboard UI       | `src/bot/keyboards.rs`                                    |
| Change project flow   | `docs/project-flow-sequence.md`                           |

---

## 📁 Directory Structure

```text
BaziFlowAgent/
├── .env                          # App secrets (never commit)
├── .env.example                  # Template for required env vars
├── Cargo.toml                    # Cargo config and dependency tree
├── GLOSSARY.md                   # Canonical domain glossary
├── prompts/                      # Embedded system prompts
├── docs/
│   ├── adr/                      # Architectural Decision Records (ADR 0001, 0002, 0003)
│   ├── agents/                   # Agent guides (issue-tracker.md, triage-labels.md, domain.md)
│   ├── issues/                   # Offline task/issue tracking directory
│   ├── project-flow-sequence.md  # Version-controlled Mermaid sequence diagrams
│   └── architecture-overview.md  # This document
├── scripts/
│   └── check.sh                  # Pre-flight formatting, clippy, and comment check
├── src/
│   ├── lib.rs                    # Library root
│   ├── main.rs                   # Entry point
│   ├── api/                      # Web API (admission.rs, handlers.rs, auth.rs)
│   ├── bot/                      # Telegram bot (commands, callbacks, keyboards, messages)
│   ├── config/                   # Configuration loader
│   ├── models/                   # Domain types, runtime admission, state
│   ├── repos/                    # Storage traits, sqlite, d1, blob_store, cache
│   └── services/                 # Bazi, almanac, conversation, llm, paipan
├── tests/                        # Integration, unit, and prototype tests
├── migrations/                   # SQLx migration files
├── logs/                         # Runtime logs (gitignored)
└── public/                       # Local HTML charts (gitignored)
```
