#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Scanning Anvil build/test processes..."
pids=()
while read -r pid command; do
  [[ -n "${pid:-}" ]] || continue
  [[ "$pid" == "$$" ]] && continue

  cwd="$(lsof -a -p "$pid" -d cwd 2>/dev/null | awk 'NR==2 {print $NF}')"
  if [[ "$cwd" == "$repo_root"* \
    || "$command" == *"$repo_root"* \
    || "$command" == *"/.cache/anvil-lang-target/"* \
    || "$command" == *"cargo test --test integration_tests"* \
    || "$command" == *"cargo test -j 1 --test integration_tests"* \
    || "$command" == *"cargo check -j 1 --bin anvil"* \
    || "$command" == *"cargo test -j 1 --bin anvil"* \
    || "$command" == *"services/proof-market/Cargo.toml"* ]]; then
    pids+=("$pid")
  fi
done < <(
  (ps -axo pid=,command= \
    | rg 'cargo (build|test|check|clippy|run)|rust-analyzer|clippy-driver|rustc|anvil-lang-target/debug|target/debug/anvil|proof-market' \
    | rg -v 'stop-anvil-builds\.sh| rg ') || true
)

if (( ${#pids[@]} == 0 )); then
  echo "No matching Anvil build/test processes found."
  exit 0
fi

printf 'Stopping PIDs:'
printf ' %s' "${pids[@]}"
printf '\n'
kill "${pids[@]}" 2>/dev/null || true
sleep 1

survivors=()
for pid in "${pids[@]}"; do
  if kill -0 "$pid" 2>/dev/null; then
    survivors+=("$pid")
  fi
done

if (( ${#survivors[@]} > 0 )); then
  printf 'Force stopping PIDs:'
  printf ' %s' "${survivors[@]}"
  printf '\n'
  kill -9 "${survivors[@]}" 2>/dev/null || true
fi

echo "Done."
