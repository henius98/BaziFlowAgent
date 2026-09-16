# Deploying BaziFlowAgent (Rust) to Raspberry Pi 4B (DietPi OS)

This guide will help you deploy the Rust Telegram bot to a Raspberry Pi running DietPi OS.

## Prerequisites

- Raspberry Pi 4B with **DietPi OS** installed.
- Internet connection on the Pi.
- SSH access (default user: `root`, pass: `dietpi`) or terminal access.
- **Rust toolchain** and **Git** installed.

### Install Build Dependencies & Rust Toolchain

On a fresh DietPi installation, you must install basic compilation tools before building Rust projects locally:

```bash
sudo apt update
sudo apt install build-essential pkg-config -y
```

Then install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
```

## Step 1: Create a Non-Root User (Optional but Recommended)

By default, DietPi uses `root`. It is safer to run the bot as a `dietpi` user.
If you are logged in as `root`:

```bash
# Check if dietpi user exists (it usually does)
id dietpi
```

Switch to the `dietpi` user or continue as is (adjust paths accordingly). This guide assumes you are using the `dietpi` user.

## Step 2: Transfer Files

### Option A: Using Git (Recommended)

1. SSH/Login as `dietpi`:
   ```bash
   su - dietpi
   ```
2. Clone your repository:
   ```bash
   git clone <your-repo-url>
   cd BaziFlowAgent
   ```

### Option B: Cross-compile on your development machine

Cross-compiling natively can be tricky due to missing C linkers. The easiest way is using `cross`:

```bash
# Install cross
cargo install cross
# Build for Raspberry Pi
cross build --release --target aarch64-unknown-linux-gnu
# Ensure target directory exists on Pi, then copy the binary
ssh dietpi@<your-pi-ip> "mkdir -p /home/dietpi/BaziFlowAgent/target/release"
scp target/aarch64-unknown-linux-gnu/release/baziflow-agent dietpi@<your-pi-ip>:/home/dietpi/BaziFlowAgent/target/release/
```

### Option C: Download Pre-built Binary (GitHub Actions - Easiest)

This repository includes a GitHub Action that automatically cross-compiles a standalone `aarch64` release binary. Everything you need is bundled in the Release — no compilation or `git clone` required.

To install or update the bot on your Raspberry Pi, simply SSH into your DietPi terminal and run this one-liner:

```bash
curl -sSL https://raw.githubusercontent.com/henius98/BaziFlowAgent/main/install.sh | bash
```

The script will automatically:
1. Download the latest `aarch64` release binary, `.env.example`, and service files.
2. Verify the checksum to ensure no corruption.
3. Prepare a new `.env` file (if one doesn't already exist).
4. Register and enable the `systemd` auto-start service.

After running the script, follow the on-screen instructions to edit your `.env` file before starting the service.

## Step 3: Build (if building on Pi)

```bash
cd /home/dietpi/BaziFlowAgent
cargo build --release
```

The binary will be located at `target/release/baziflow-agent`. 
*(Note: Initial compilation on a Raspberry Pi 4 may take 15-20 minutes depending on SD card speed.)*

## Step 4: Configure Environment Variables

The bot requires multiple environment variables (LLM API Keys, Database URL, Cron schedules, etc.) to function properly.

1. Copy the example `.env` file:
   ```bash
   cp .env.example .env
   ```
2. Edit the `.env` file:
   ```bash
   nano .env
   ```
3. **Mandatory updates**:
   - `TELEGRAM_BOT_TOKEN`: Your bot token from @BotFather.
   - `LLM_API_KEY`: Your AI provider key (e.g., Gemini, OpenAI).
   - `BASE_URL`: The public URL/IP (and port) of your DietPi so Instant View charts can load (e.g., `http://192.168.1.100:8080`).
4. **Optional Cloudflare R2 Storage (For Chart Images)**:
   - If using HTML charts via Cloudflare R2, fill in `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, and `R2_BUCKET_NAME`.
5. Save and exit (`Ctrl+O`, `Enter`, `Ctrl+X`).

## Step 5: Test Manually

```bash
# Run from the project directory (so .env is loaded)
./target/release/baziflow-agent
```

- Send `/start` to your bot.
- The bot will automatically create the SQLite database and run any necessary migrations on startup.
- Check the terminal for any panics or database creation issues.
- `Ctrl+C` to stop.

## Step 6: Set Up Systemd Service (Auto-start)

The repository includes a pre-configured `BaziFlowAgent.service` file. We will use this instead of creating a new one.

1. **Verify paths in service file**:
   Check `BaziFlowAgent.service` to ensure `WorkingDirectory` and `ExecStart` match your setup (default is `/home/dietpi/BaziFlowAgent`).

2. **Copy service file to systemd** (requires sudo/root):
   ```bash
   sudo cp BaziFlowAgent.service /etc/systemd/system/BaziFlowAgent.service
   ```

3. **Enable and Start**:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable BaziFlowAgent
   sudo systemctl start BaziFlowAgent
   ```

4. **Check Status**:
   ```bash
   sudo systemctl status BaziFlowAgent
   ```

## Updating the Service

If you modify the code and rebuild:

1. **Build the new binary:**
   ```bash
   cargo build --release
   ```
   The service automatically uses the new binary when restarted!

2. **Restart the Service:**
   ```bash
   sudo systemctl restart BaziFlowAgent
   ```

## Troubleshooting

- **Logs**:
  ```bash
  sudo journalctl -u BaziFlowAgent -f
  ```

- **Enable debug logging**:
  Change `LOG_LEVEL=debug` in your `.env` file, or run manually:
  ```bash
  LOG_LEVEL=debug ./target/release/baziflow-agent
  ```

## Integration deployment changes

The HTTP listener now defaults to `127.0.0.1:8080`. Terminate HTTPS/WSS at your reverse proxy and forward WebSocket upgrades. Restrict any non-loopback `HTTP_BIND_ADDRESS` to that proxy. Enforce connection/IP limits, header/handshake deadlines, and downstream write timeouts at the proxy; the application's request admission begins after HTTP headers arrive. Keep the proxy WebSocket idle timeout above `WS_IDLE_SECONDS` and disable SSE buffering.

Run database migrations with a backup available. Migration checksum mismatches now stop startup; the application never deletes `_sqlx_migrations` to hide a mismatch. The new migration adds a chart capability column/index. Regenerate local charts to replace old predictable URLs; do not independently serve `public/` from the proxy. Treat chart paths as credentials and exclude them from access logs. Existing chart files are retained.

SIGTERM and Ctrl+C initiate bounded draining. Keep systemd's `TimeoutStopSec` above `SHUTDOWN_SECONDS` (default 30 seconds). Deadline expiry is logged; unfinished work can be lost. Recent chat history remains a disposable cache. Old database logs may still contain prompts and responses from previous versions; apply your retention policy to that historical data. New LLM log bodies are redacted.

The production constraints, owner permissions, reconnect semantics, and configuration defaults are documented in [WebSocket integration](docs/websocket-integration.md).

A reviewable [nginx example](deploy/nginx.conf.example) includes WSS upgrade headers, TLS termination, per-IP handshake/connection limits, SSE settings, and timeouts. Replace its hostname and certificate paths, align its body/time limits with application configuration, and run `nginx -t` on the deployment host before enabling it. This repository change does not deploy the proxy or provision certificates.
