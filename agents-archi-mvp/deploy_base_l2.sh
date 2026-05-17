#!/usr/bin/env bash
# [Ω] deploy_base_l2.sh — ArchiLedger deployment to Base Mainnet / Sepolia
# Usage:
#   ./deploy_base_l2.sh              # Base Sepolia (testnet)
#   ./deploy_base_l2.sh mainnet      # Base Mainnet (REAL MONEY)
#
# Requires:
#   ARCHI_PRIVATE_KEY env var set to deployer wallet private key
#   Deployer wallet must have ETH on the target network
set -euo pipefail

NETWORK="${1:-sepolia}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Validate private key
if [ -z "${ARCHI_PRIVATE_KEY:-}" ]; then
    echo "[!] Error: ARCHI_PRIVATE_KEY environment variable not set."
    echo "    export ARCHI_PRIVATE_KEY=0x..."
    exit 1
fi

# Set RPC URL based on network
if [ "$NETWORK" = "mainnet" ]; then
    RPC_URL="${BASE_RPC_URL:-https://mainnet.base.org}"
    CHAIN_ID=8453
    EXPLORER="https://basescan.org"
    echo ""
    echo "============================================"
    echo " [!] WARNING: DEPLOYING TO BASE MAINNET"
    echo "     This uses REAL ETH. Proceed? (y/N)"
    echo "============================================"
    read -r confirm
    if [ "$confirm" != "y" ]; then
        echo "Aborted."
        exit 0
    fi
elif [ "$NETWORK" = "sepolia" ]; then
    RPC_URL="${BASE_SEPOLIA_RPC_URL:-https://sepolia.base.org}"
    CHAIN_ID=84532
    EXPLORER="https://sepolia.basescan.org"
else
    echo "[!] Unknown network: $NETWORK"
    echo "    Usage: $0 [sepolia|mainnet]"
    exit 1
fi

echo ""
echo "[Ω] ArchiLedger L2 Deployment"
echo "    Network:  Base $NETWORK (Chain ID: $CHAIN_ID)"
echo "    RPC:      $RPC_URL"
echo "    Explorer: $EXPLORER"
echo ""

cd "$SCRIPT_DIR"

# Build first
echo "[*] Compiling contracts..."
forge build

# Deploy
echo "[*] Deploying ArchiLedger to Base $NETWORK..."
OUTPUT=$(forge create \
    --rpc-url "$RPC_URL" \
    --private-key "$ARCHI_PRIVATE_KEY" \
    --broadcast \
    contracts/ArchiLedger.sol:ArchiLedger 2>&1)

echo "$OUTPUT"

# Extract deployed address
DEPLOYED_ADDR=$(echo "$OUTPUT" | grep "Deployed to:" | awk '{print $3}')
TX_HASH=$(echo "$OUTPUT" | grep "Transaction hash:" | awk '{print $3}')

if [ -z "$DEPLOYED_ADDR" ]; then
    echo "[!] Deployment failed. Could not extract contract address."
    exit 1
fi

echo ""
echo "============================================"
echo " [C5-REAL] ArchiLedger DEPLOYED"
echo "    Address:  $DEPLOYED_ADDR"
echo "    Tx Hash:  $TX_HASH"
echo "    Verify:   $EXPLORER/address/$DEPLOYED_ADDR"
echo "============================================"

# Save config
cat > "$SCRIPT_DIR/deployed_address.json" <<EOF
{
    "contract_address": "$DEPLOYED_ADDR",
    "rpc_url": "$RPC_URL",
    "tx_hash": "$TX_HASH",
    "network": "base_$NETWORK",
    "network_id": $CHAIN_ID,
    "reality_level": "C5-REAL",
    "explorer": "$EXPLORER/address/$DEPLOYED_ADDR"
}
EOF

echo "[+] Configuration saved to deployed_address.json"
echo ""
echo "[Ω] To verify on Basescan:"
echo "    forge verify-contract $DEPLOYED_ADDR contracts/ArchiLedger.sol:ArchiLedger --chain-id $CHAIN_ID --watch"
