#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JOBS="${CARGO_BUILD_JOBS:-1}"
HOST="127.0.0.1"
PORT="${ANVIL_SMOKE_PORT:-}"
HEALTH_URL=""
SAAS_LOG="$(mktemp "${TMPDIR:-/tmp}/anvil-smoke-saas.XXXXXX")"
SAAS_PID=""

export CARGO_BUILD_JOBS="$JOBS"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}"

cleanup() {
  if [[ -n "$SAAS_PID" ]] && kill -0 "$SAAS_PID" 2>/dev/null; then
    kill "$SAAS_PID" 2>/dev/null || true
    wait "$SAAS_PID" 2>/dev/null || true
  fi
  rm -f "$SAAS_LOG"
}

trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

step() {
  printf '\n==> %s\n' "$*"
}

run() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run_integration_tests() {
  local filters=(
    build_
    parse_
    regression_
    soundness_assert
    soundness_bitflow
    soundness_block
    soundness_build
    soundness_fncall
    soundness_hex
    soundness_i128
    soundness_impossible
    soundness_large
    soundness_loop
    soundness_mixed
    soundness_nested
    soundness_no
    soundness_shadowing
    soundness_signed
    soundness_std
    soundness_u
    soundness_vacuous
    verify_
  )

  local filter
  for filter in "${filters[@]}"; do
    run cargo test -j "$JOBS" --test integration_tests "$filter" -- --test-threads=1
  done
}

wait_for_health() {
  local attempt
  for attempt in {1..40}; do
    if [[ -n "$SAAS_PID" ]] && ! kill -0 "$SAAS_PID" 2>/dev/null; then
      echo "SaaS process exited before health check passed." >&2
      return 1
    fi

    if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
      return 0
    fi

    sleep 0.25
  done

  echo "Timed out waiting for $HEALTH_URL" >&2
  return 1
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

choose_port() {
  if [[ -n "$PORT" ]]; then
    ensure_port_free "$PORT"
    HEALTH_URL="http://${HOST}:${PORT}/health"
    return
  fi

  PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("", 0))
    print(sock.getsockname()[1])
PY
)"
  HEALTH_URL="http://${HOST}:${PORT}/health"
}

ensure_port_free() {
  python3 - "$1" <<'PY'
import socket
import sys

port = int(sys.argv[1])

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    try:
        sock.bind(("", port))
    except OSError as exc:
        print(f"port {port} is not available: {exc}", file=sys.stderr)
        sys.exit(1)
PY
}

target_dir() {
  cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

validate_health() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if payload.get("status") != "online":
    raise SystemExit(f"unexpected health status: {payload!r}")
if payload.get("engine") != "Anvil+Z3 Formal Verification":
    raise SystemExit(f"unexpected health engine: {payload!r}")
if not payload.get("version"):
    raise SystemExit(f"missing health version: {payload!r}")
PY
}

validate_doctor_json() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if payload.get("ok") is not True:
    raise SystemExit(f"doctor reported failure: {payload!r}")
if not payload.get("checks"):
    raise SystemExit(f"doctor checks missing: {payload!r}")
PY
}

validate_check_json() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if payload.get("ok") is not True:
    raise SystemExit(f"check reported failure: {payload!r}")
if payload.get("status") != "VERIFIED":
    raise SystemExit(f"unexpected check status: {payload!r}")
if payload.get("timeout_ms") != 5000:
    raise SystemExit(f"timeout_ms missing or wrong: {payload!r}")
PY
}

cd "$ROOT_DIR"

require_tool cargo
require_tool curl
require_tool python3

step "Cargo check"
run cargo check -j "$JOBS" --bin anvil

step "Unit tests"
run cargo test -j "$JOBS" --bin anvil

step "Integration tests"
run_integration_tests

step "CLI JSON diagnostics"
run cargo build -j "$JOBS" --bin anvil
ANVIL_BIN="$(target_dir)/debug/anvil"

printf '+ %q doctor --json\n' "$ANVIL_BIN"
DOCTOR_JSON="$("$ANVIL_BIN" doctor --json)"
validate_doctor_json "$DOCTOR_JSON"

printf '+ %q check --json --timeout 5000 examples/transfer.anv\n' "$ANVIL_BIN"
CHECK_JSON="$("$ANVIL_BIN" check --json --timeout 5000 examples/transfer.anv)"
validate_check_json "$CHECK_JSON"

step "SaaS health"
choose_port

printf '+ %q saas --port %q\n' "$ANVIL_BIN" "$PORT"
"$ANVIL_BIN" saas --port "$PORT" >"$SAAS_LOG" 2>&1 &
SAAS_PID="$!"

if ! wait_for_health; then
  echo
  echo "SaaS log:"
  sed -n '1,160p' "$SAAS_LOG" >&2
  exit 1
fi

HEALTH_BODY="$(curl -fsS "$HEALTH_URL")"
validate_health "$HEALTH_BODY"
printf '%s' "$HEALTH_BODY"
echo

step "Smoke suite passed"
