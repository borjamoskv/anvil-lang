# Anvil 🔨

**A programming language where trust doesn't compile.**

> Every function carries its proof. If the compiler can't prove your invariants, your code doesn't exist.

## What is Anvil?

Anvil is a formally verified programming language designed for **smart contracts** and **autonomous agents**. It uses [Z3](https://github.com/Z3Prover/z3) (Microsoft's SMT solver) to mathematically prove that your code satisfies its invariants at compile time.

**No tests. No assertions. No "trust me". Just proof.**

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
- `sender_balance >= amount` — precondition
- `sender_balance' + receiver_balance'` — the `'` notation means "after execution" (post-state)
- The compiler verifies that the function body satisfies ALL invariants

## Usage

```bash
# Verify a file
anvil check examples/transfer.anv

# Compile to Rust
anvil build examples/transfer.anv -o transfer.rs

# Dump AST
anvil ast examples/transfer.anv
```

## Architecture

```
.anv source → [Pest Parser] → AST → [Z3 Verifier] → [Codegen] → .rs
                                          ↓
                                    SAT? → ✗ REJECTED (counterexample shown)
                                    UNSAT? → ✓ PROVEN (code generated)
```

## Why?

Every new programming language is born from an unbearable friction:

- **Fortran**: "I can't keep writing assembly"
- **Rust**: "Memory bugs kill people"
- **Anvil**: "I can't prove my smart contract is correct without deploying it and losing millions"

The K2 Lending close-factor bypass. The DAO hack. The Wormhole exploit. All would have been **impossible** in Anvil — the compiler would have rejected the code before it existed.

## Status

**v0.1 — Proof of Concept**

- [x] PEG Grammar (pest)
- [x] Parser → AST
- [x] Z3 SMT Verification Engine
- [x] Rust Code Generation
- [x] CLI (check / build / ast)
- [ ] Type checker
- [ ] Loop invariant verification
- [ ] Contract-level invariants
- [ ] LSP / Editor support
- [ ] LLVM backend

## License

MIT

---

*"A language where `trust me` doesn't compile."*
*Created by BorjaMoskv × Antigravity, May 2026.*
