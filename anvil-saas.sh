#!/usr/bin/env bash
# anvil-saas.sh — Start Anvil Proof Market SaaS locally
set -euo pipefail

PORT=${1:-4242}
Z3_LIB="${Z3_LIB:-/opt/homebrew/Cellar/z3/4.15.4/lib}"
BINARY=${ANVIL_BIN:-}
BINARY_FROM_ENV=0

if [ -z "$BINARY" ]; then
  TARGET_DIR=${CARGO_TARGET_DIR:-}

  if [ -z "$TARGET_DIR" ]; then
    TARGET_DIR=$(
      cargo metadata --no-deps --format-version 1 \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
    )
  fi

  if [ -z "$TARGET_DIR" ]; then
    echo "Could not determine Cargo target directory" >&2
    exit 1
  fi

  BINARY="$TARGET_DIR/release/anvil"
else
  BINARY_FROM_ENV=1
fi

if [ ! -x "$BINARY" ]; then
  if [ "$BINARY_FROM_ENV" -eq 1 ]; then
    echo "ANVIL_BIN is not executable: $BINARY" >&2
    exit 1
  fi
  echo "→ Binary not found. Building..."
  JOBS=${CARGO_BUILD_JOBS:-1}
  CARGO_BUILD_JOBS="$JOBS" cargo build --release -j "$JOBS" --bin anvil
fi

echo "→ Starting Anvil SaaS on http://localhost:$PORT"
echo "→ Prometheus metrics: http://localhost:$PORT/metrics"
echo "→ Health: http://localhost:$PORT/health"
echo "→ Portal: http://localhost:$PORT/"
echo ""

DYLD_LIBRARY_PATH="$Z3_LIB${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" "$BINARY" saas --port "$PORT"
