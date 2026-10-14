# AI Bazi Telegram Bot Agent: AI Developer Context

Quick-start index and operating rules for AI coding agents working on BaziFlowAgent.

**Core Rules:**
- DO NOT REMOVE USER COMMENTS DURING EDITS. (`<RULE[user_global]>`)
- Always Keep Single Source of Truth.
- Never commit code.

---

## 🧭 Navigation Pointers

Detailed reference is disclosed across dedicated documents to minimize turn context load:

- **Architecture, Code Map & Tech Stack**: See [docs/architecture-overview.md](docs/architecture-overview.md).
- **Project Flow & Request Sequences**: See [docs/project-flow-sequence.md](docs/project-flow-sequence.md).
- **Architectural Decision Records**: See [docs/adr/](docs/adr/) (ADR 0001 Storage Traits, ADR 0002 Zero-Copy, ADR 0003 Admission Control, ADR 0004 Workspace & Trait Seams).
- **Domain Terminology**: See [GLOSSARY.md](GLOSSARY.md) and [docs/agents/domain.md](docs/agents/domain.md).
- **Issue Tracker & Offline Fallback**: See [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md).
- **Triage Labels**: See [docs/agents/triage-labels.md](docs/agents/triage-labels.md).

---

## 🧠 Architectural & Implementation Guidelines

1. **Async Contexts & Lifetimes:**
   - Always `.clone()` `Arc` bindings, SQLite pools, and `DashMap` instances before moving into `async` closures. Use `DashMap` to avoid `Mutex` deadlocks across high-traffic Telegram handlers.

2. **Storage & Decoupling:**
   - Relational database access uses concrete `Database` enum and `ChatHistoryCache` directly, while object storage uses `BlobStore`. Never couple domain handlers directly to raw pool primitives.

3. **Database Migrations:**
   - New schema changes require a new file in `./migrations/`. Use `sqlx::query!` compile-time macros where possible.

4. **Astrology Specifics (Critical):**
   - Strictly follows "Blindman Bazi" methodology (体用 Ti Yong, 做功 Zuo Gong).
   - **Do NOT** introduce generic Ziping (子平旺衰 balance theory) logic unless explicitly requested.
   - Refer to `prompts/BaziHuangLiAssistant.md` for exact AI parameters — embedded at compile time via `include_str!`.

5. **Timezones:**
   - The bot is configured around SGT/CST (UTC+8). Use `chrono-tz` for strict midnight roll-overs when fetching next-day almanac data.

6. **Errors & Safety Invariants:**
   - Use `tracing::info!`, `tracing::warn!`, and `tracing::error!`.
   - Use `AppResult<T>` / `LogErrorExt` from `models/error.rs` for error propagation.
   - **No `unwrap()` Policy:** `.unwrap()` is banned in production logic. Use `.expect("reason")` only if mathematically provable or use `?` operator.

7. **Project Flow & Sequence Updates:**
   - Any change adding, removing, or changing request entry points, handlers, services, datastores, or streaming protocols MUST update `docs/project-flow-sequence.md` in the same change.

---

## 🛡️ Agent Execution Guardrails

1. **No Background Polling Loops:**
   - Do NOT poll `manage_task(action="status")` repeatedly after starting background commands (`cargo test`, `cargo build`). Stop turns immediately and rely on reactive system wakeup notifications.

2. **Inspect Before Edit:**
   - Never perform blind `replace_file_content` edits based on assumed line numbers. Always view the exact lines with `view_file` immediately before applying replacement chunks.

3. **Mandatory Pre-Flight Check:**
   - Before completing tasks, always execute `./scripts/check.sh` to ensure `cargo fmt`, `cargo check`, `cargo clippy`, all tests, and comment preservation pass with zero errors.
