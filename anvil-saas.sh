#!/usr/bin/env bash
# anvil-saas.sh — Start Anvil Proof Market SaaS locally
set -e

PORT=${1:-4242}
Z3_LIB="/opt/homebrew/Cellar/z3/4.15.4/lib"
BINARY="./target/release/anvil"

if [ ! -f "$BINARY" ]; then
  echo "→ Binary not found. Building..."
  cargo build --release
fi

echo "→ Starting Anvil SaaS on http://localhost:$PORT"
echo "→ Prometheus metrics: http://localhost:$PORT/metrics"
echo "→ Health: http://localhost:$PORT/health"
echo "→ Portal: http://localhost:$PORT/"
echo ""

DYLD_LIBRARY_PATH="$Z3_LIB" "$BINARY" saas --port "$PORT"
