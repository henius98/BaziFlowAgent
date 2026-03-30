# Deploying DifyBaziWorkflow (Rust) to Raspberry Pi 4B (DietPi OS)

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

By default, DietPi uses `root`. It is safer to run the bot as `dietpi` user.
If you are logged in as `root`:

```bash
# Check if dietpi user exists (it usually does)
id dietpi
```

Switch to `dietpi` user or continue as is (adjust paths accordingly). This guide assumes you are using the `dietpi` user.

## Step 2: Transfer Files

### Option A: Using Git (Recommended)
1.  SSH/Login as `dietpi`:
    ```bash
    su - dietpi
    ```
2.  Clone your repository:
    ```bash
    git clone <your-repo-url>
    cd DifyBaziWorkflow
    ```

### Option B: Cross-compile on your development machine
```bash
# Install the ARM target
rustup target add aarch64-unknown-linux-gnu
# Build for Raspberry Pi
cargo build --release --target aarch64-unknown-linux-gnu
# Copy the binary
scp target/aarch64-unknown-linux-gnu/release/dify-telegram-bot dietpi@<your-pi-ip>:/home/dietpi/DifyBaziTelegramBot/
```

## Step 3: Build (if building on Pi)

```bash
cd /home/dietpi/DifyBaziWorkflow
cargo build --release
```

The binary will be at `target/release/dify-telegram-bot`.

## Step 4: Configure Environment Variables

1.  Create `.env`:
    ```bash
    nano .env
    ```
2.  Paste your secrets:
    ```env
    TOKEN=your_telegram_bot_token_here
    DIFY_WEBHOOK_URL=your_dify_webhook_url_here
    ```
3.  Save and exit (`Ctrl+O`, `Enter`, `Ctrl+X`).

## Step 5: Test Manually

```bash
# Run from the project directory (so .env is loaded)
./target/release/dify-telegram-bot
```
- Send `/start` to your bot.
- `Ctrl+C` to stop.

## Step 6: Set Up Systemd Service (Auto-start)

1.  **Copy binary to deployment directory**:
    ```bash
    # (Optional) Verify your .env is present
    ls -la /home/dietpi/DifyBaziWorkflow/.env
    ```

2.  **Copy service file to systemd** (requires sudo/root):
    ```bash
    sudo cp telegramBot.service /etc/systemd/system/telegramBot.service
    ```

3.  **Enable and Start**:
    ```bash
    sudo systemctl daemon-reload
    sudo systemctl enable telegramBot
    sudo systemctl start telegramBot
    ```

4.  **Check Status**:
    ```bash
    sudo systemctl status telegramBot
    ```

## Updating the Service

If you modify the code and rebuild:

1.  **Build the new binary:**
    ```bash
    cargo build --release
    ```

    The service automatically uses the new binary when restarted!

3.  **Restart the Service:**
    ```bash
    sudo systemctl restart telegramBot
    ```

## Troubleshooting

- **Logs**:
    ```bash
    sudo journalctl -u telegramBot -f
    ```

- **Enable debug logging**:
    ```bash
    RUST_LOG=debug ./target/release/dify-telegram-bot
    ```
