#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source "$repo_root/scripts/lowmem-env.sh"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: scripts/proof-market-local.sh

Starts the standalone local Proof Market page at http://127.0.0.1:8000/.
It builds the local Anvil CLI, points ANVIL_BIN at Cargo's target directory,
and enables mock payment explicitly for demo use.

Environment:
  PROOF_MARKET_ADDR              Bind address (default: 127.0.0.1:8000)
  ANVIL_CERTIFICATE_SECRET       Certificate secret (default: local-dev-secret)
  ANVIL_ALLOW_MOCK_PAYMENT       Enable mock mode (default: 1)
  ANVIL_PROCESS_TIMEOUT_SECS     Subprocess timeout (default: 10)
  ANVIL_PROCESS_MEMORY_MB        Linux/Android memory cap in MB (default: 512)
USAGE
  exit 0
fi

target_dir() {
  cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool cargo
require_tool python3

echo "-> Building local Anvil CLI and Proof Market sidecar"
cargo build -j 1 --bin anvil
cargo build -j 1 --manifest-path services/proof-market/Cargo.toml --bin proof-market

export ANVIL_BIN="${ANVIL_BIN:-$(target_dir)/debug/anvil}"
export PROOF_MARKET_ADDR="${PROOF_MARKET_ADDR:-127.0.0.1:8000}"
export ANVIL_CERTIFICATE_SECRET="${ANVIL_CERTIFICATE_SECRET:-local-dev-secret}"
export ANVIL_ALLOW_MOCK_PAYMENT="${ANVIL_ALLOW_MOCK_PAYMENT:-1}"
export ANVIL_PROCESS_TIMEOUT_SECS="${ANVIL_PROCESS_TIMEOUT_SECS:-10}"
export ANVIL_PROCESS_MEMORY_MB="${ANVIL_PROCESS_MEMORY_MB:-512}"

if [[ ! -x "$ANVIL_BIN" ]]; then
  echo "ANVIL_BIN is not executable: $ANVIL_BIN" >&2
  exit 1
fi

echo "-> Proof Market page: http://${PROOF_MARKET_ADDR}/"
echo "-> Health: http://${PROOF_MARKET_ADDR}/health"
echo "-> ANVIL_BIN: $ANVIL_BIN"
echo "-> Mock payment: $ANVIL_ALLOW_MOCK_PAYMENT"
echo ""

exec cargo run -j 1 --manifest-path services/proof-market/Cargo.toml
