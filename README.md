# Anvil 🔨

[![CI](https://github.com/borjamoskv/anvil-lang/actions/workflows/ci.yml/badge.svg)](https://github.com/borjamoskv/anvil-lang/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-0.6.0-blue)](https://github.com/borjamoskv/anvil-lang/releases)
[![License: Sovereign](https://img.shields.io/badge/License-Sovereign-2B3BE5.svg)](SOVEREIGN_LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)](https://www.rust-lang.org/)

**A programming language where trust doesn't compile.**

> Every function carries its proof. If the compiler can't prove your invariants, your code doesn't exist.

## What is Anvil?

Anvil is a formally verified programming language designed for **smart contracts** and **autonomous agents**. It uses [Z3](https://github.com/Z3Prover/z3) (Microsoft's SMT solver) to mathematically prove that your code satisfies its invariants at compile time.

**No tests. No assertions. No "trust me". Just proof.**

## Why Formal Verification?

Every major DeFi exploit was a violated invariant that nobody checked:

| Exploit | Loss | Root Cause | Anvil Prevention |
| :--- | :--- | :--- | :--- |
| **The DAO** (2016) | $60M | Reentrancy: state update after external call | Postcondition `balance' == balance - amount` forces correct ordering |
| **Wormhole** (2022) | $320M | Missing signature validation | Precondition `is_valid_signature == true` enforced at compile time |
| **K2 Lending** (2026) | Close factor bypass | Sequential liquidation calls exceed 50% limit | `total_repaid' <= debt * close_factor / 100` proven by Z3 |

Anvil doesn't just *test* for these bugs — it makes them **mathematically impossible**.

## Quick Example

```anvil
fn transfer(sender_balance: u64, receiver_balance: u64, amount: u64) -> u64
    where {
        amount > 0,
        sender_balance >= amount,
        sender_balance' + receiver_balance' == sender_balance + receiver_balance,
        sender_balance' == sender_balance - amount
    }
{
    sender_balance -= amount;
    receiver_balance += amount;
    return sender_balance;
}
```

The `where` clause declares invariants that Z3 **must prove** at compile time:
- `amount > 0` — precondition
- `sender_balance >= amount` — precondition (prevents underflow)
- `sender_balance' + receiver_balance'` — the `'` notation means "after execution" (post-state)
- The compiler verifies that the function body satisfies ALL invariants

## Usage

```bash
# Verify a file (parse → typecheck → Z3 prove)
anvil check examples/transfer.anv

# Emit machine-readable check results for CI/API/frontend integrations
anvil check --json examples/transfer.anv

# Compile to Rust (only if verified)
anvil build examples/transfer.anv -o transfer.rs

# Dump AST as JSON
anvil ast examples/transfer.anv
```

## Architecture

```text
.anv source → [Pest Parser] → AST → [Type Checker] → [Z3 Verifier] → [Codegen] → .rs / .ll
                                          ↓                  ↓                       ↓
                                  Type constraints     SAT? → ✗ REJECTED     LLVM IR (eBPF/RISC-V/x86)
                                  injected into Z3     UNSAT? → ✓ PROVEN     Rust (zero runtime checks)
```

### Verification Pipeline

1. **Parse** — PEG grammar via `pest` crate → typed AST
2. **Type Check** — Bidirectional inference, overflow detection, type constraints registered for Z3 (e.g., `u64 → var ≥ 0 ∧ var < 2^64`)
3. **Z3 Verify** — Hoare logic with frame rule:
   - Assert preconditions
   - Encode body effects (assignments → Z3 equations)
   - Apply frame rule (unmodified vars: post == pre)
   - Check postconditions (negate and check SAT)
4. **Codegen** — Transpile to Rust (zero runtime checks) or emit LLVM IR (Direct-Silicon JIT for eBPF/RISC-V/x86)

## v0.6 Language Features

### `assumes` — Environment Axioms

```anvil
fn transfer(from: Wallet, to: Wallet, amount: u256) -> u256
    assumes {
        from != to     // Trusted — not proven by Z3
    }
    where {
        amount > 0     // This IS proven
    }
{ ... }
```

### Ghost Variables — Proof-Domain Only

```anvil
fn swap(reserve_x: u256, reserve_y: u256, amount_in: u256) -> u256
    where { reserve_x' * reserve_y' >= reserve_x * reserve_y }
{
    ghost k_before: u256 = reserve_x * reserve_y;  // Exists in Z3, not in binary
    reserve_x += amount_in;
    // ...
}
```

### Sovereign Types & Events

```anvil
fn verify(signer: Wallet, hash: TxHash, sig: Signature, gas: Gas) -> bool
    where { gas > 0 }
{
    emit Verified(signer, hash);
    return true;
}
```

## Examples

| File | Description | Expected |
| :--- | :--- | :--- |
| `transfer.anv` | Token transfer with conservation law | ✅ PROVEN |
| `vault.anv` | ERC-4626 deposit/withdraw with `&&`/`\|\|` | ✅/✅/❌ (broken withdraw caught) |
| `reentrancy.anv` | The DAO hack pattern | ❌ REJECTED (counterexample shown) |
| `overflow.anv` | Integer overflow attack | ✅ PROVEN with bounds |
| `token.anv` | ERC-20 with contract-level invariant | ✅/✅/❌ (accounting equation violated) |
| `ssa.anv` | Sequential assignments (SSA) | ✅/✅/✅/❌ (broken sequence caught) |
| `loops.anv` | Mathematical proofs (3-way conservation) | ✅/✅/✅/❌/❌ |
| `while_loops.anv` | While loops with inductive invariants | ✅/✅/❌ (broken countdown caught) |

## Status

**v0.6 — Sovereign Types + Quantifiers + Direct-Silicon JIT**

- [x] PEG Grammar (pest)
- [x] Parser → AST
- [x] Z3 SMT Verification Engine (Hoare Logic + Frame Rule)
- [x] Type Checker (bidirectional inference, bounded integers, overflow detection)
- [x] Type constraints → Z3 (silicon-bounded verification)
- [x] Rust Code Generation (zero runtime checks)
- [x] **LLVM IR Backend** (if/else/while control flow, assert→trap, eBPF/RISC-V/x86)
- [x] CLI (check / build / ast)
- [x] Adversarial test suite (reentrancy, overflow, share inflation)
- [x] Logical operators (`&&`, `||`) in invariant expressions
- [x] Contract-level invariants (global accounting equations)
- [x] SSA body encoding (sequential assignments)
- [x] While loop verification (havoc-assume-exit pattern)
- [x] If-else handling (branch overapproximation)
- [x] **Sovereign Types** (`Wallet`, `Signature`, `TxHash`, `Gas`)
- [x] **`assumes` Clause** (environment axioms)
- [x] **Ghost Variables** (proof-domain bindings)
- [x] **`emit` Statement** (on-chain events)
- [x] **Quantifier Translation** (`forall`/`exists` → Z3)
- [x] **On-Chain Standard Library** (`std/onchain.anv`)
- [ ] LSP / Editor support
- [ ] Macro system (compile-time metaprogramming)
- [ ] Arena memory model (O(1) tensor-state)

## The Thermodynamic Thesis

> *The current path of a computation: Python → PyTorch → C++ → CUDA → Kernel → GPU → Silicon.*
> *Each arrow is friction. Each layer is heat. Each abstraction is a lie.*
>
> *Anvil collapses the stack: Mathematical Proof → Verified Code → Silicon.*
> *The `where` clause IS the contract. The body is a temporary biological compromise.*
> *When Direct-Silicon JIT arrives, the body disappears. Only the constraints remain.*

## License

Anvil operates under the **[Sovereign License v1.0](SOVEREIGN_LICENSE.md)**.
- **Anvil Core** (Parser, AST, Type Checker) is Open Source (MIT).
- **Anvil Sovereign Engine** (Z3 Verification, Direct-Silicon JIT) is proprietary.

For commercial proofs and high-performance execution, access the **Proof Market** at `agents.archi`.

## Quick Start

→ **[Getting Started Guide](docs/getting-started.md)** — Install, verify, and write your first invariant.

→ **[Z3 Installation](docs/z3-installation.md)** — Platform-specific Z3 setup instructions.

→ **[SaaS Guide](docs/saas-guide.md)** — Run the Proof Market API with Prometheus metrics.

→ **Local Proof Market demo** — Run `scripts/proof-market-local.sh`, open `http://127.0.0.1:8000/`, paste `.anv`, use mock payment, and inspect the certificate or counterexample.

→ **[`anvil check --json`](docs/saas-guide.md#anvil-check---json-schema-v1)** — Stable schema v1 for CI, API, and frontend integrations.

## Contributing

Contributions welcome. See **[CONTRIBUTING.md](CONTRIBUTING.md)** for development setup, code style, and PR workflow.

---

*"A language where `trust me` doesn't compile."*
*Created by BorjaMoskv × Antigravity, May 2026.*
