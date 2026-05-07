# Anvil 🔨

**A programming language where trust doesn't compile.**

> Every function carries its proof. If the compiler can't prove your invariants, your code doesn't exist.

## What is Anvil?

Anvil is a formally verified programming language designed for **smart contracts** and **autonomous agents**. It uses [Z3](https://github.com/Z3Prover/z3) (Microsoft's SMT solver) to mathematically prove that your code satisfies its invariants at compile time.

**No tests. No assertions. No "trust me". Just proof.**

## Why Formal Verification?

Every major DeFi exploit was a violated invariant that nobody checked:

| Exploit | Loss | Root Cause | Anvil Prevention |
|---|---|---|---|
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

# Compile to Rust (only if verified)
anvil build examples/transfer.anv -o transfer.rs

# Dump AST as JSON
anvil ast examples/transfer.anv
```

## Architecture

```
.anv source → [Pest Parser] → AST → [Type Checker] → [Z3 Verifier] → [Codegen] → .rs
                                          ↓                  ↓
                                  Type constraints     SAT? → ✗ REJECTED
                                  injected into Z3     UNSAT? → ✓ PROVEN
```

### Verification Pipeline

1. **Parse** — PEG grammar via `pest` crate → typed AST
2. **Type Check** — Bidirectional inference, overflow detection, type constraints registered for Z3 (e.g., `u64 → var ≥ 0 ∧ var < 2^64`)
3. **Z3 Verify** — Hoare logic with frame rule:
   - Assert preconditions
   - Encode body effects (assignments → Z3 equations)
   - Apply frame rule (unmodified vars: post == pre)
   - Check postconditions (negate and check SAT)
4. **Codegen** — Transpile to Rust with zero runtime checks (invariants already proven)

## Examples

| File | Description | Expected |
|---|---|---|
| `transfer.anv` | Token transfer with conservation law | ✅ PROVEN |
| `vault.anv` | ERC-4626 deposit/withdraw | ✅/✅/❌ (broken withdraw caught) |
| `reentrancy.anv` | The DAO hack pattern | ❌ REJECTED (counterexample shown) |
| `overflow.anv` | Integer overflow attack | ❌ REJECTED by type constraints |

## Status

**v0.2 — Type Checker + Adversarial Suite**

- [x] PEG Grammar (pest)
- [x] Parser → AST
- [x] Z3 SMT Verification Engine (Hoare Logic + Frame Rule)
- [x] **Type Checker** (bidirectional inference, bounded integers, overflow detection)
- [x] **Type constraints → Z3** (silicon-bounded verification)
- [x] Rust Code Generation
- [x] CLI (check / build / ast)
- [x] **Adversarial test suite** (reentrancy, overflow, share inflation)
- [ ] Loop invariant verification
- [ ] Contract-level invariants
- [ ] LSP / Editor support
- [ ] LLVM backend

## The Thermodynamic Thesis

> *The current path of a computation: Python → PyTorch → C++ → CUDA → Kernel → GPU → Silicon.*
> *Each arrow is friction. Each layer is heat. Each abstraction is a lie.*
>
> *Anvil collapses the stack: Mathematical Proof → Verified Code → Silicon.*
> *The `where` clause IS the contract. The body is a temporary biological compromise.*
> *When Direct-Silicon JIT arrives, the body disappears. Only the constraints remain.*

## License

MIT

---

*"A language where `trust me` doesn't compile."*
*Created by BorjaMoskv × Antigravity, May 2026.*
