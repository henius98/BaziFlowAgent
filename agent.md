# Dify Bazi Telegram Bot: AI Developer Context

This file serves as a quick-start index for AI agents (like Cursor, Windsurf, or Copilot) to understand the project structure and context immediately, saving tokens and indexing time.

**Core Rule:** DO NOT REMOVE USER COMMENTS DURING EDITS. (`<RULE[user_global]>`)

## 🛠 Tech Stack Overview
- **Language:** Rust (Edition 2024)
- **Bot Framework:** `teloxide` (macros enabled)
- **Async Framework:** `tokio` (full features)
- **Database:** `sqlx` (SQLite, async with `runtime-tokio-rustls`)
- **Web API & Scraping:** `reqwest`, `axum`
- **Concurrency / State:** `dashmap` (for in-memory user sessions)
- **Job Scheduling:** `tokio-cron-scheduler` (daily almanac pulls)
- **Time Handling:** `chrono`, `chrono-tz`
- **Logging:** `tracing`, `tracing-subscriber`

## 📂 Source Code Map (`src/`)

- **`main.rs`**: Entry point. Sets up the SQLite connection pool (`db::init_db()`), initializes `tracing`, and creates the `teloxide` bot instance. Manages the main `DashMap` for user session concurrency and registers Telegram handlers (commands, inline queries, and messages).
- **`db.rs`**: Database abstraction layer using `sqlx`. Handles user interaction logs, requests telemetry, and manages SQL migrations which run on bot startup. (Migrations are located in `./migrations/`).
- **`api_extract.rs`**: The logical core for fetching the MingDecode Almanac API. Parses JSON payloads, extracts exact Bazi/astrology metrics, and calculates custom astrology components like "Kong Wang" (空亡).
- **`calendar.rs`**: Generates complex, dynamic inline Telegram keyboard calendars for users to pick dates in chat.
- **`dify.rs`**: Webhook and HTTP client handler that interfaces with the Dify AI platform. It merges the user's input, the selected calendar dates, and constructs prompts formatted for a Chain of Thought (CoT) process.
- **`scheduler.rs`**: Asynchronous job handling using `tokio-cron-scheduler`. Automates batched operations, like generating next-day astrology reports at specific intervals (e.g., 10 PM SGT) and pruning stale database / DashMap entries.

## 🧠 Architectural & Implementation Guidelines

1. **Async Contexts & Lifetimes:** 
   - Always map shared state (`Arc` bindings, SQLite pools, `DashMap` instances) correctly. Use `.clone()` before moving these instances into `async` block tasks or closures. Use `DashMap` to avoid standard Mutex lock deadlocks across high-traffic Telegram handlers.

2. **Database Migrations:** 
   - Modifying schema requires a new file in the `./migrations/` folder. All SQL interactions in `db.rs` should use compile-time validated `sqlx::query!` macros where possible.
   - Run `cargo sqlx prepare` if you change SQL queries (if offline checking is enabled), or ensure `cargo check` validates the schema.

3. **Astrology Specifics (Critical):**
   - The bot evaluates destiny strictly adhering to standard "Blindman Bazi" methods (体用 - Ti Yong, 做功 - Zuo Gong). 
   - Do NOT introduce generic Ziping (子平旺衰 - Balance theory) logic unless explicitly requested. 
   - Refer to `BaziHuangLiAssistantPrompt.md` for exact AI parameters.

4. **Timezones:** 
   - The bot is configured around the Singapore/Beijing timezone (SGT/CST, UTC+8). Use `chrono-tz` to ensure strict roll-overs at local midnight when fetching the next day's Almanac API.

5. **Errors & Logging:**
   - Print state updates using `tracing::info!`, `tracing::warn!`, and `tracing::error!` directly. Panic gracefully, but prioritize returning clean error messages via Telegram contexts instead of halting the runtime.

## ⚡ Quick Flow for New Features

- **Adding a Command:** Update `main.rs` Telegram command enums and dispatchers.
- **Modifying State:** Insert/Retrieve from `DashMap` in `main.rs` or update SQLite structures in `db.rs`.
- **API Call Logic:** Append functionality into `api_extract.rs` or `dify.rs`.
- **Scheduled Task:** Register a new Cron expression inside `scheduler.rs`.

*Note: The `py_src/` folder contains legacy Python code and shouldn't be executed or updated unless explicitly bridging legacy algorithms.*
