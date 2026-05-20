#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

source "$repo_root/scripts/lowmem-env.sh"

target_dir() {
  cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

target="${CARGO_TARGET_DIR:-$(target_dir)}"

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
    cargo test -j 1 --test integration_tests "$filter" -- --test-threads=1
  done
}

cargo build -j 1 --bin anvil
cargo build -j 1 --manifest-path services/proof-market/Cargo.toml --bin proof-market
cargo test -j 1 --manifest-path services/proof-market/Cargo.toml -- --test-threads=1
run_integration_tests
ANVIL_BIN="$target/debug/anvil" PROOF_MARKET_BIN="$target/debug/proof-market" python3 tests/test_proof_market.py
ANVIL_BIN="$target/debug/anvil" PROOF_MARKET_BIN="$target/debug/proof-market" python3 tests/test_proof_market_amm.py
cargo run -j 1 --bin anvil -- check examples/hello.anv
cargo run -j 1 --bin anvil -- check examples/transfer.anv
cargo build -j 1 --release --bin anvil
cargo build -j 1 --release --manifest-path services/proof-market/Cargo.toml --bin proof-market
