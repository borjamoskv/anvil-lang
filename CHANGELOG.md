# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2026-05-09

### Added
- **Sovereign Types:** `Wallet` (32-byte), `Signature` (65-byte), `TxHash` (32-byte), `Gas` (u64) as first-class language primitives
- **`assumes` Clause:** Environment axioms the verifier trusts without proving (e.g., `from != to` for transfers)
- **Ghost Variables:** Proof-domain-only bindings (`ghost k: u256 = x * y;`) — exist in Z3 but stripped from codegen
- **`emit` Statement:** On-chain event emission (`emit Transfer(from, to, amount);`)
- **Quantifier Translation:** `forall`/`exists` now translate to live Z3 `forall_const`/`exists_const` (previously dead code)
- **LLVM Control Flow:** `if/else` → `br i1`/labels, `while` → loop header/body/exit blocks
- **LLVM Assert → Trap:** `assert` compiles to `call void @llvm.trap()` on failure path
- **LLVM Target Triples:** Header supports eBPF, RISC-V, x86_64 target selection
- **Standard Library:** `src/std/onchain.anv` — `transfer`, `verify_and_execute`, `gas_bounded_swap`, `multi_sig_approve` (all 4 verified, 6/6 postconditions proven)
- **Tests:** 4 new parser tests (sovereign_types, assumes_clause, emit_statement, ghost_statement)

### Changed
- LLVM types: `u128` → `i128`, `u256` → `i256` (was incorrectly mapped to `i64`)
- Sovereign types mapped to native LLVM: `Wallet`/`TxHash` → `i256`, `Signature` → `ptr`, `Gas` → `i64`
- Codegen: `Address` → `[u8; 20]` (was `Address`), sovereign types → native Rust byte arrays
- `gen_llvm_function` refactored: statement emission extracted to `gen_llvm_stmt` for recursive control flow
- Version bump from 0.5.1 to 0.6.0

### Fixed
- `MulAssign` and `DivAssign` now emit correct LLVM IR (were previously `; Unsupported assign op`)
- `is_unsigned()` now includes `Gas` type

## [0.5.1] - 2026-05-09

### Added
- **CI/CD:** Matrix builds (stable + MSRV 1.80), `cargo audit` security scanning, doc generation
- **CI/CD:** Tag-triggered release workflow with automatic GitHub Releases
- **Observability:** `tracing` integration across all pipeline stages (parser, typechecker, verifier, codegen)
- **Observability:** Prometheus metrics on SaaS `/metrics` endpoint (`anvil_verify_requests_total`, `anvil_verify_duration_seconds`, `anvil_verify_result`)
- **Docs:** Hello World example (`examples/hello.anv`)
- **Docs:** Getting Started tutorial (`docs/getting-started.md`)
- **Docs:** Z3 Installation guide (`docs/z3-installation.md`)
- **Docs:** SaaS execution guide (`docs/saas-guide.md`)
- **Community:** `CONTRIBUTING.md`, issue templates (bug report, feature request), PR template
- **Metadata:** `CHANGELOG.md`, Cargo.toml with repository/homepage/keywords

### Changed
- CI workflow split into `lint`, `build_and_test` (matrix), `integration`, and `security` jobs
- `setup-python` action updated from v4 to v5
- SaaS health endpoint now returns version from `Cargo.toml` instead of hardcoded string

### Fixed
- Version number in CLI banner now correctly references v0.5.1

## [0.5.0] - 2026-05-08

### Added
- While loop verification with havoc-assume-exit pattern
- If-else branch overapproximation
- Loop exit condition assertions
- SSA body encoding for sequential assignments
- Contract-level invariants (global accounting equations)
- Logical operators (`&&`, `||`) in invariant expressions
- Bidirectional type inference with silicon-bounded integers
- Type constraints injected into Z3 (u8/u16/u32/u64/u128/u256 bounds)
- Overflow/underflow detection warnings
- LLVM IR code generation backend
- Language Server Protocol (LSP) support
- Proof Market SaaS API with Axum
- SHA3-256 proof hashing for CORTEX provenance
- Adversarial test suite (reentrancy, overflow, share inflation)
- Global conservation and inflation defense invariants

## [0.4.0] - 2026-05-07

### Added
- Rust code generation from verified AST
- CLI with `check`, `build`, `ast` subcommands
- PEG grammar via pest crate

## [0.3.0] - 2026-05-06

### Added
- Z3 SMT Verification Engine (Hoare Logic + Frame Rule)
- Precondition/postcondition classification
- Counterexample extraction from SAT models

## [0.2.0] - 2026-05-05

### Added
- Parser → typed AST
- PEG grammar for Anvil syntax

## [0.1.0] - 2026-05-04

### Added
- Initial project scaffold
- Anvil language concept and design
