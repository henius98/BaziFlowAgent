# BaziFlowAgent ☯️🤖

**Try the live demo on Telegram:** [@BaziFlowAgent_bot](https://t.me/BaziFlowAgent_bot)

<p align="center">
  <img src="./logo_ai_bot.png" alt="BaziFlowAgent Logo" width="300" />
</p>

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Teloxide](https://img.shields.io/badge/telegram-teloxide-blue.svg)](https://github.com/teloxide/teloxide)

A high-performance Telegram Bot built in **Rust** 🦀 that provides professional Daily Almanac (黄历) & Bazi (八字) fortune-telling analysis.

## 🌟 Key Features

- **Interactive Calendar UI**: Features a custom-built inline Telegram keyboard calendar for picking dates to evaluate.
- **Daily Almanac API Integration**: Fetches traditional Chinese almanac data (MingDecode API), keeping only essential variables, calculating "Kong Wang" (空亡), and translating keys dynamically.
- **LLM AI Native**: Automatically structures system prompts (based on Blindman Bazi methodology) alongside chat contexts, injecting calendar selections to a remote LLM for sophisticated CoT (Chain of Thought) analysis, delivered via real-time token streaming to Telegram.
- **Scheduled Analytics**: Built-in async job scheduler (`tokio-cron-scheduler`) dynamically triggers daily report calculations based on individual user schedules, proactively informing you about tomorrow's astrological landscape. Features customizable schedule settings and enhanced user profile views.
- **Robust Concurrency**: Leverages `tokio` and `DashMap` for memory-safe, lock-free concurrency to maintain isolated user contexts.
- **Strict Rust Quality Standards**: Enforces Microsoft Pragmatic Guidelines, Zero `.unwrap()` error handling architectures, and memory-safe Clean Architecture patterns.

## 🏗️ Architecture Stack

- **Framework**: `teloxide` (Telegram Bot)
- **Runtime**: `tokio` (Async runtime)
- **Requests Engine**: `reqwest` + `serde_json`
- **Task Scheduling**: `tokio-cron-scheduler`
- **Memory Storage**: InMemory `DashMap` (Self-cleaning stale sessions automatically)
- **API Trigger**: Minimal Axum server for external system orchestration.

## 📁 Repository Structure

```text
BaziFlowAgent/
├── .env                          # App secrets (never commit)
├── .env.example                  # Template for required env vars
├── Cargo.toml                    # Cargo config and dependency tree
├── DEPLOYMENT.md                 # ARM/Raspberry Pi deployment guide
├── BaziFlowAgent.service         # Systemd unit file
├── prompts/                      # Embedded system prompts
│   ├── BaziHuangLiAssistant.md   # System prompt for daily readings
│   ├── BaziSummaryAssistant.md   # System prompt for Bazi summarization
│   ├── DateSelectionAssistant.md # System prompt for date selection (/pick)
│   ├── FollowUpAssistant.md      # System prompt for free-text follow-ups
│   └── UserBaziAssistant.md      # System prompt for destiny readings
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

## 🚀 Getting Started

### 1. Requirements

- Rust toolchain (run `rustup update`)
- Telegram Bot Token (from [@BotFather](https://t.me/BotFather))
- OpenAI-compatible LLM API Key (e.g., OpenAI, Gemini, etc.)

### 2. Environment Variables

We provide a template for all required environment variables, including database configuration, LLM parameters, and cron schedules.

1. Copy the example file:
   ```bash
   cp .env.example .env
   ```
2. Open `.env` and fill in your secrets (e.g., `TELEGRAM_BOT_TOKEN`, `LLM_API_KEY`).

### 3. Build & Run locally

```bash
# Build the project
cargo build --release

# Run the Bot
cargo run --release
```

## ⚙️ Deployment

Are you deploying on an ARM-based edge device like a **Raspberry Pi 4B (DietPi OS)**? Check out the comprehensive **[DEPLOYMENT.md](./DEPLOYMENT.md)** guide to cleanly install, cross-compile, and set up your systemd daemon!

## 🧠 Native LLM Prompt Design

The bot uses `async-openai` to natively orchestrate LLM calls. It strictly enforces the constraints and prompts specified in `prompts/`. This prevents generic responses (e.g., ziping "旺衰" theory) and mandates the Blindman Bazi "体用" & "做功" methodology. The Almanac data and User Intent are constructed and provided directly within the LLM messages.
