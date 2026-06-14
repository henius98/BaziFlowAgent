#!/bin/bash
set -e

# ==============================================================================
# BaziFlowAgent One-Step Deployment Script
# Downloads the latest release binary and sets up the systemd service.
# Run this directly on your Raspberry Pi (DietPi OS).
# ==============================================================================

echo "==========================================="
echo "   BaziFlowAgent Deployment Script         "
echo "==========================================="

REPO_URL="https://github.com/henius98/BaziFlowAgent"
RELEASE_TAG="latest"
BASE_DIR="/home/dietpi/BaziFlowAgent"
BIN_DIR="$BASE_DIR/target/release"

echo "[1/5] Setting up directories..."
mkdir -p "$BIN_DIR"
cd "$BASE_DIR"

echo "[2/5] Downloading latest release assets..."
wget -q --show-progress "$REPO_URL/releases/download/$RELEASE_TAG/baziflow-agent" -O "$BIN_DIR/baziflow-agent"
wget -q --show-progress "$REPO_URL/releases/download/$RELEASE_TAG/baziflow-agent.sha256" -O "$BIN_DIR/baziflow-agent.sha256"
wget -q --show-progress "$REPO_URL/releases/download/$RELEASE_TAG/BaziFlowAgent.service" -O BaziFlowAgent.service

echo "[3/5] Verifying checksum..."
cd "$BIN_DIR"
sha256sum -c baziflow-agent.sha256 || { echo "Checksum verification failed!"; exit 1; }
chmod +x baziflow-agent
cd "$BASE_DIR"

echo "[4/5] Configuring environment..."
if [ ! -f .env ]; then
    echo "WARNING: .env file not found!"
    echo "You must manually create $BASE_DIR/.env and add your configuration."
    NEEDS_CONFIG=1
else
    echo ".env file already exists."
    NEEDS_CONFIG=0
fi

echo "[5/5] Setting up systemd service..."
sudo chown -R dietpi:dietpi "$BASE_DIR"
sudo cp BaziFlowAgent.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable BaziFlowAgent.service

if [ "$NEEDS_CONFIG" -eq 1 ]; then
    echo "==========================================="
    echo "Deployment downloaded successfully!"
    echo "ACTION REQUIRED: You MUST create a .env file with your tokens before starting!"
    echo "  nano $BASE_DIR/.env"
    echo "Then start the bot with:"
    echo "  sudo systemctl start BaziFlowAgent"
else
    sudo systemctl restart BaziFlowAgent
    echo "==========================================="
    echo "Deployment completed and service restarted successfully!"
    echo "Check logs with: sudo journalctl -fu BaziFlowAgent"
fi
