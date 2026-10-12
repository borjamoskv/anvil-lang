#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo_root/scripts/lowmem-env.sh"
exec python3 "$repo_root/scripts/ci_runner.py" smoke "$@"
