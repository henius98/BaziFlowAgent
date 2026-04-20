# BaziFlowAgent ☯️🤖

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Teloxide](https://img.shields.io/badge/telegram-teloxide-blue.svg)](https://github.com/teloxide/teloxide)

A high-performance Telegram Bot built in **Rust** 🦀 that integrates with **BaziFlowAgent** to provide professional Daily Almanac (黄历) & Bazi (八字) fortune-telling analysis.

## 🌟 Key Features

- **Interactive Calendar UI**: Features a custom-built inline Telegram keyboard calendar for picking dates to evaluate.
- **Daily Almanac API Integration**: Fetches traditional Chinese almanac data (MingDecode API), keeping only essential variables, calculating "Kong Wang" (空亡), and translating keys dynamically.
- **LLM AI Native**: Automatically structures system prompts (based on Blindman Bazi methodology) alongside chat contexts, injecting calendar selections to a remote LLM for sophisticated CoT (Chain of Thought) analysis.
- **Scheduled Analytics**: Built-in async job scheduler (`tokio-cron-scheduler`) triggers a daily report calculation at 10 PM SGT, proactively informing you about tomorrow's astrological landscape.
- **Robust Concurrency**: Leverages `tokio` and `DashMap` for memory-safe, lock-free concurrency to maintain isolated user contexts.

## 🏗️ Architecture Stack

- **Framework**: `teloxide` (Telegram Bot)
- **Runtime**: `tokio` (Async runtime)
- **Requests Engine**: `reqwest` + `serde_json`
- **Task Scheduling**: `tokio-cron-scheduler`
- **Memory Storage**: InMemory `DashMap` (Self-cleaning stale sessions automatically)

## 📁 Repository Structure

```text
BaziFlowAgent/
├── Cargo.toml                  # Rust dependencies & package config
├── DEPLOYMENT.md               # Instructions for DietPi/Raspberry Pi setup
├── telegramBot.service         # Systemd unit file for background running
├── BaziHuangLiAssistantPrompt.md # Blindman Bazi system instructions for LLM Node
├── src/                        # 🦀 Active Rust Source Code
│   ├── main.rs                 # Telegram Bot entry point and logic hub
│   ├── api_extract.rs          # API processor & Kong Wang calculations
│   ├── calendar.rs             # Custom inline calendar component
│   ├── llm.rs                 # llm webhook integration client
│   └── scheduler.rs            # Daily & self-cleanup Cron scheduler
```

## 🚀 Getting Started

### 1. Requirements

- Rust toolchain (run `rustup update`)
- Telegram Bot Token (from [@BotFather](https://t.me/BotFather))
- Your deployed AI Agent instance webhook endpoint URL

### 2. Environment Variables

Create a `.env` file at the root of the project representing your secrets:

```env
TELEGRAM_BOT_TOKEN=your_telegram_bot_token_here
```

### 3. Build & Run locally

```bash
# Build the project
cargo build --release

# Run the Bot
cargo run --release
```

## ⚙️ Deployment

Are you deploying on an ARM-based edge device like a **Raspberry Pi 4B (DietPi OS)**? Check out the comprehensive **[DEPLOYMENT.md](./DEPLOYMENT.md)** guide to cleanly install, cross-compile, and set up your systemd daemon!

## 🧠 AI Agent Workflow Prompt Design

If you are assembling the logic inside the AI Agent AI canvas, you must use the constraints and prompts specified in the `BaziHuangLiAssistantPrompt.md`. It strictly prevents generic responses (e.g., ziping "旺衰" theory) and mandates the Blindman Bazi "体用" & "做功" methodology. Provide the Almanac data and User Intent directly within AI Agent's inputs.

---

_Open sourced for the Bazi & Developer community._
