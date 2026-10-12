# anvil-lang

> Formally Verified DSL & Runtime Environment with SMT/Z3 Proof Guarantees

`anvil-lang` is a domain-specific language designed for deterministic agent execution and formal verification of computational contracts.

## Key Features

- **SMT/Z3 Integration**: Automated formal verification of execution invariants prior to runtime.
- **Type-Safe Contract System**: Prevents runtime state corruption and non-deterministic behavior.
- **Zero-Anergy Memory Management**: Minimal cognitive and computational overhead.

## Architecture

Built as a lightweight computational kernel for agentic protocol verification.

## Local Checks

Run from the repository checkout; the scripts resolve the workspace from their
own location. Cargo uses `target/`, shared by Anvil and proof-market, with one
build job and no debug symbols or incremental compilation in dev/test profiles.
`CARGO_TARGET_DIR` and the other Cargo environment variables remain overridable.
On Apple Silicon, the default native paths use Homebrew's stable `opt/z3` and
`opt/sqlite` links. Linux CI supplies its system library paths through environment
variables.

```sh
scripts/ci-quick.sh
tools/smoke.sh
scripts/ci-full.sh

scripts/ci-full.sh --list
scripts/ci-full.sh --phase market-http
scripts/ci-full.sh --phase integration-soundness
scripts/ci-full.sh --from release-anvil
```

An unfiltered run executes all phases. Selected phases include their build
prerequisites. Integration tests are discovered from Cargo's compiled test
executable and run once each in exact-name batches, including unfamiliar names;
`--batch-size` controls the maximum tests per process (default: eight).

Each run preserves output, timing, peak RSS measured with `wait4` on macOS/Linux,
and exit codes in `cache/ci/`. `summary.json` identifies the last phase and overall
result, while `events.jsonl` records each command's start and completion. An
unfinished `running` entry means the process did not report completion. SIGKILL
and SIGTERM are reported separately; a signal alone does not establish memory
exhaustion. Set `ANVIL_CI_LOG_DIR` to store logs elsewhere. SaaS logs are retained
and the server is stopped after the health check.

Use `scripts/clean-builds.sh --dry-run` to inspect package-scoped cleanup when
needed; cache cleanup is not required before normal checks.
