#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

source "$repo_root/scripts/lowmem-env.sh"

cargo fmt -- --check
cargo fmt --manifest-path services/proof-market/Cargo.toml -- --check
cargo check -j 1 --bin anvil
cargo check -j 1 --manifest-path services/proof-market/Cargo.toml
cargo test -j 1 --bin anvil -- --test-threads=1
cargo test -j 1 --manifest-path services/proof-market/Cargo.toml -- --test-threads=1
