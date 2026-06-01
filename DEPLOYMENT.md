# Deploying BaziFlowAgent (Rust) to Raspberry Pi 4B (DietPi OS)

This guide will help you deploy the Rust Telegram bot to a Raspberry Pi running DietPi OS.

## Prerequisites

- Raspberry Pi 4B with **DietPi OS** installed.
- Internet connection on the Pi.
- SSH access (default user: `root`, pass: `dietpi`) or terminal access.
- **Rust toolchain** and **Git** installed.

### Install Rust Toolchain

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
# Copy the binary to your Pi
scp target/aarch64-unknown-linux-gnu/release/baziflow-agent dietpi@<your-pi-ip>:/home/dietpi/BaziFlowAgent/
```

## Step 3: Build (if building on Pi)

```bash
cd /home/dietpi/BaziFlowAgent
cargo build --release
```

The binary will be located at `target/release/baziflow-agent`.

## Step 4: Configure Environment Variables

The bot requires multiple environment variables (LLM API Keys, Database URL, Cron schedules, etc.) to function properly.

1. Copy the example `.env` file:
   ```bash
   cp .env.example .env
   ```
2. Edit the `.env` file and paste your secrets:
   ```bash
   nano .env
   ```
3. Update `TELEGRAM_BOT_TOKEN`, `LLM_API_KEY`, and ensure `DATABASE_URL` is set correctly.
4. Save and exit (`Ctrl+O`, `Enter`, `Ctrl+X`).

## Step 5: Test Manually

```bash
# Run from the project directory (so .env is loaded)
./target/release/baziflow-agent
```

- Send `/start` to your bot.
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
