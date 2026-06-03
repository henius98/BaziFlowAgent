# AI Bazi Telegram Bot Agent: AI Developer Context

This file serves as a quick-start index for AI agents (like Cursor, Windsurf, or Copilot) to understand the project structure and context immediately, saving tokens and indexing time.

**Core Rule:** DO NOT REMOVE USER COMMENTS DURING EDITS. (`<RULE[user_global]>`)

---

## 🌐 Project Overview

A high-performance Telegram Bot built in **Rust** providing professional Chinese Daily Almanac (黄历) and Bazi (八字) fortune-telling analysis. Instead of relying solely on hardcoded logic, it intelligently retrieves traditional calendar data via an external API, formats it, and natively orchestrates requests to an **LLM service (via OpenAI-compatible endpoints)** using specialized "Blindman Bazi" (盲派命理) prompts to generate Chain-of-Thought (CoT) analysis.

---

## 🛠 Tech Stack Overview

| Layer         | Library                                 | Purpose                                     |
| ------------- | --------------------------------------- | ------------------------------------------- |
| Bot Framework | `teloxide` (macros enabled)             | Telegram Bot API & routing via `dptree`     |
| Async Runtime | `tokio` (full features)                 | Multi-threaded async reactor                |
| HTTP Client   | `reqwest`, `async-openai`               | External APIs & OpenAI-compatible LLM calls |
| Database      | `sqlx` (SQLite, `runtime-tokio-rustls`) | Persist users, sessions, and requests       |
| Concurrency   | `dashmap`                               | Lock-free in-memory user session state      |
| Scheduling    | `tokio-cron-scheduler`                  | Daily almanac pulls & session GC            |
| Time          | `chrono`, `chrono-tz`                   | Timezone-aware date handling (SGT/UTC+8)    |
| Logging       | `tracing`, `tracing-subscriber`         | Structured async-safe logging               |
| Serialization | `serde`, `serde_json`                   | JSON parsing for API payloads               |
| Web Server    | `axum`, `tower-http`                    | Static file serving for Instant View        |
| Object Storage| `rust-s3`                               | Cloudflare R2 integration for HTML charts   |

---

## 📂 Source Code Map (`src/`)

### Entry Point & Infrastructure
- **`lib.rs`**: Library root declaring all public modules for external access or testing.
- **`main.rs`**: Loads `AppConfig` from env, sets up the SQLite pool, creates the `reqwest` HTTP client, builds `AppState`, registers Telegram handlers via `dptree`, starts the scheduler, and runs the axum static file server.
- **`logger.rs`**: Initialises `tracing-subscriber` with dual console + file output (daily rotation). Includes `cleanup_old_logs()` for retention.
- **`scheduler.rs`**: Background cron jobs: (1) dynamic daily fortune readings for users with an active schedule, (2) user context expiration cleanup, (3) log file retention cleanup.
- **`utils.rs`**: Shared utility functions used by both `bot/` and `scheduler.rs` — `split_message()` for Telegram message chunking and `get_formatted_bazi_four_pillars()` for JSON→prompt conversion.

### `bot/` — Telegram Bot Handlers & UI
- **`mod.rs`**: Module declarations and `Command` re-export.
- **`commands.rs`**: `Command` enum (`/new`, `/date`, `/pick`, `/profile`, `/model`, `/schedule`) and `handle_command` handler.
- **`callbacks.rs`**: `handle_callback` — dispatches all inline keyboard callback queries across 9 namespaces (Date, Birthdate, Gender, Location, Time, Model, Pick Date, Pick Activity, Schedule).
- **`messages.rs`**: `handle_message` — free-text message handler with conversation context tracking.
- **`command_actions.rs`**: `perform_bazi_analysis` & `display_user_profile` — Telegram UI orchestration for the `/new` birthdate flow, and user profile management (including schedule settings).
- **`helpers.rs`**: Bot-specific helpers — `build_history_msg()`, `get_username()`, and LLM token streaming (`stream_to_telegram()`).
- **`keyboards.rs`**: Dynamic inline Telegram keyboard builders for calendars, year/month/day/hour/minute pickers, gender selector, location picker, model picker, and schedule settings.

### `config/` — Configuration
- **`mod.rs`**: `AppConfig` struct — reads and validates all configuration from `.env` via `dotenvy`. Single source of truth for secrets and tunables.

### `models/` — Domain Types
- **`mod.rs`**: Module declarations and re-exports.
- **`error.rs`**: `AppError` enum, `AppResult<T>` type alias, and `LogErrorExt` trait for ergonomic error logging.
- **`state.rs`**: `AppState` struct — shared via `Arc` across all handlers. Holds HTTP client, SQLite pool, config, and `DashMap`-based pending state for the `/new` flow.
- **`common.rs`**: Common data types and enums (e.g., `LlmModel` for AI selection).

### `repos/` — Database Layer
- **`mod.rs`**: SQLite initialization (`init_db`), user CRUD, request logging, and Bazi profile queries.

### `services/` — Domain Logic (Bot-Framework-Agnostic)
- **`mod.rs`**: Module declarations and re-exports.
- **`almanac.rs`**: Fetches raw calendar data from MingDecode API, applies a schema filter, recursively translates English JSON keys to Chinese labels, and computes "Kong Wang" (空亡).
- **`bazi_service.rs`**: Pure business logic orchestrator for Bazi generation (True Solar Time calculation, API fetching, DB persistence, HTML generation & Cloudflare R2 upload with presigned URLs, and LLM prompt construction).
- **`llm.rs`**: General-purpose LLM client utilizing `async-openai`. Includes `LlmRequestParams` mapping to builder pattern, with database logging.
- **`solar_time.rs`**: True Solar Time (真太阳时) calculation using Equation of Time and longitude adjustment. Includes city longitude lookup.
- **`paipan/`**: Bazi chart module:
  - `mod.rs` — Re-exports the submodule interfaces.
  - `bazi_utils.rs` — Helper functions and utilities for intermediate Bazi calculations.
  - `client.rs` — API client fetching base chart + supplementary data (relations, geju, yongshi, liunian) with concurrent requests.
  - `models.rs` — `BaziChart`, `StructuredBazi`, and related serde structs.
  - `formatter.rs` — Transforms raw chart data into structured JSON for LLM prompts and HTML diagram generation.
  - `bazi_template.html` — HTML template for Bazi chart visualization.

---

## 🔄 Overall Request Flow

```
Telegram User
    │
    ▼
teloxide Dispatcher (dptree)
    ├─ /new, /date, /pick... ──► bot::commands::handle_command
    │                                  ├─ /new, /date, /pick, /model, /schedule → build respective inline keyboards (bot::keyboards)
    │                                  └─ /profile → bot::command_actions::display_user_profile
    ├─ Callback Query ─────────► bot::callbacks::handle_callback
    │                                  ├─ Calendar/Time navigation  → rebuild keyboards
    │                                  ├─ Date selected             → services::almanac / services::llm based reading
    │                                  └─ Birthtime selected        → bot::command_actions::perform_bazi_analysis
    └─ Free-text Message ──────► bot::messages::handle_message
                                       └─ services::llm call for analysis

services::llm::call_llm
    ├─ async-openai → LLM endpoint       (system prompt + user bazi + almanac + history)

scheduler (background)
    ├─ dynamic user crons    → personalized daily readings for users with active schedules
    ├─ context_cleanup_cron  → evict stale user_contexts + user_last_active
    └─ log_cleanup_cron      → remove old log files
```

---

## 🧠 Architectural & Implementation Guidelines

1. **Async Contexts & Lifetimes:**
   - Always `.clone()` `Arc` bindings, SQLite pools, and `DashMap` instances before moving into `async` closures. Use `DashMap` to avoid `Mutex` deadlocks across high-traffic Telegram handlers.

2. **Database Migrations:**
   - New schema changes require a new file in `./migrations/`. Use `sqlx::query!` compile-time macros where possible.
   - Run `cargo sqlx prepare` after changing SQL queries if offline checking is enabled.

3. **Astrology Specifics (Critical):**
   - Strictly follows "Blindman Bazi" methodology (体用 Ti Yong, 做功 Zuo Gong).
   - **Do NOT** introduce generic Ziping (子平旺衰 balance theory) logic unless explicitly requested.
   - Refer to `prompts/BaziHuangLiAssistant.md` for exact AI parameters — it is embedded at compile time via `include_str!`.

4. **Timezones:**
   - The bot is configured around SGT/CST (UTC+8). Use `chrono-tz` for strict midnight roll-overs when fetching next-day almanac data.

5. **Errors & Logging:**
   - Use `tracing::info!`, `tracing::warn!`, and `tracing::error!`. Return clean error messages to users via Telegram rather than panicking.
   - Use `AppResult<T>` / `LogErrorExt` from `models/error.rs` for consistent error propagation.
   - **No `unwrap()` Policy:** `.unwrap()` is banned in production logic to maintain safety invariants. Use `.expect("reason")` only if mathematically provable or use the `?` operator to bubble up errors.

6. **Module Boundaries:**
   - `services/` must remain bot-framework-agnostic — no Telegram types here.
   - `bot/` owns all Telegram-specific code (keyboards, handlers, callbacks).
   - `utils.rs` holds shared functions needed by both `bot/` and `scheduler.rs`.

7. **Rust Quality & Memory Safety:**
   - Enforce Clean Architecture separation. Minimize `clone()` arbitrarily; restructure lifetimes or use references (`&T`) when possible. Maintain Eval-Driven verification loops for robust feature development.
---

## ⚡ Quick Feature Reference

| Task                  | Where to edit                                               |
| --------------------- | ----------------------------------------------------------- |
| Add a bot command     | `bot/commands.rs` `Command` enum + `handle_command` match   |
| Add callback handling | `bot/callbacks.rs` — new namespace dispatch                 |
| Modify shared state   | `models/state.rs` `AppState` struct                         |
| Add a DB table/column | New file in `./migrations/` + `repos/mod.rs`                |
| Change API parsing    | `services/almanac.rs` `KEEP_SCHEMA` / `KEY_MAP`             |
| Tweak LLM parameters  | `services/llm.rs` `LlmRequestParams`                        |
| Add a scheduled job   | `scheduler.rs` — new `Job::new_async(cron, ...)`            |
| Change configuration  | `config/mod.rs` `AppConfig` + `.env`                        |
| Add keyboard UI       | `bot/calendar.rs`                                           |

---

## 📄 Important Documentation Files

| File                            | Purpose                                                            |
| ------------------------------- | ------------------------------------------------------------------ |
| `README.md`                     | Top-level intro, feature list, env vars, build/run commands        |
| `prompts/`                      | Directory containing all system prompts (e.g. Daily readings, Destiny readings, Summarization, Date selection) |
| `DEPLOYMENT.md`                 | Raspberry Pi / DietPi ARM cross-compilation & systemd daemon setup |
| `BaziFlowAgent.service`        | Pre-configured systemd unit file for background deployment         |
| `Cargo.toml`                    | Canonical dependency list and package metadata                     |
| `.env` / `.env.example`         | Runtime secrets and configurables (never commit `.env`)            |

---

## 📁 Directory Structure

```text
BaziFlowAgent/
├── .env                          # App secrets (never commit)
├── .env.example                  # Template for required env vars
├── Cargo.toml                    # Cargo config and dependency tree
├── DEPLOYMENT.md                 # ARM/Raspberry Pi deployment guide
├── BaziFlowAgent.service         # Systemd unit file
├── prompts/                      # Embedded system prompts (HuangLi, UserBazi, Summary, Pick, FollowUp)
├── src/
│   ├── lib.rs                    # Library root declaring public modules
│   ├── main.rs                   # Entry point & bot wiring
│   ├── logger.rs                 # Tracing init & log cleanup
│   ├── scheduler.rs              # Background cron jobs
│   ├── utils.rs                  # Shared utilities (split_message, etc.)
│   ├── bot/
│   │   ├── mod.rs                # Module declarations
│   │   ├── commands.rs           # /new, /date, /pick, /profile, /model, /schedule command handlers
│   │   ├── callbacks.rs          # Callback query dispatcher
│   │   ├── messages.rs           # Free-text message handler
│   │   ├── command_actions.rs    # /new birthdate Telegram UI orchestration
│   │   ├── helpers.rs            # Bot-specific helper functions
│   │   └── keyboards.rs          # Inline keyboard UI builders
│   ├── config/
│   │   └── mod.rs                # AppConfig (env loader)
│   ├── models/
│   │   ├── mod.rs                # Re-exports
│   │   ├── common.rs             # Common data types (e.g., LlmModel)
│   │   ├── error.rs              # AppError, AppResult, LogErrorExt
│   │   └── state.rs              # AppState struct
│   ├── repos/
│   │   └── mod.rs                # SQLite DB layer
│   └── services/
│       ├── mod.rs                # Module declarations
│       ├── almanac.rs            # MingDecode API + Kong Wang calc
│       ├── bazi_service.rs       # Core business logic for Bazi generation
│       ├── llm.rs                # General LLM client mapping & logging
│       ├── solar_time.rs         # True Solar Time calculation
│       └── paipan/
│           ├── mod.rs            # Re-exports
│           ├── bazi_utils.rs     # Helper utilities for Bazi calculations
│           ├── client.rs         # Bazi chart API client
│           ├── formatter.rs      # Chart → JSON/HTML formatting
│           ├── models.rs         # Bazi data structures
│           └── bazi_template.html
├── migrations/                   # SQLx migration SQL files
├── logs/                         # Runtime logs (gitignored)
└── public/                       # Runtime HTML charts (gitignored)
```
