#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source "$repo_root/scripts/lowmem-env.sh"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: scripts/clean-builds.sh [cargo clean filter flags]

Package-scoped cleanup for Anvil build artifacts. This preserves the shared
Cargo target directory and avoids deleting unrelated cached builds.
The script chooses package and manifest paths itself; do not pass -p/--package
or --manifest-path.

Examples:
  scripts/clean-builds.sh --dry-run
  scripts/clean-builds.sh
  scripts/clean-builds.sh --release
USAGE
  exit 0
fi

for arg in "$@"; do
  case "$arg" in
    -p|--package|--manifest-path|--package=*|--manifest-path=*)
      echo "error: scripts/clean-builds.sh sets package and manifest filters itself" >&2
      exit 2
      ;;
  esac
done

cargo clean --package anvil --manifest-path Cargo.toml "$@"
cargo clean --package proof-market --manifest-path services/proof-market/Cargo.toml "$@"
