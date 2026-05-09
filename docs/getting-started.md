# Getting Started with Anvil

This guide walks you through installing Anvil, verifying your first program, and understanding the output.

## Prerequisites

- **Rust** ≥ 1.80 ([Install Rust](https://rustup.rs/))
- **Z3 SMT Solver** ≥ 4.8 (see [Z3 Installation Guide](z3-installation.md))

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/borjamoskv/anvil-lang.git
cd anvil-lang

# Build
cargo build --release

# Verify the installation
./target/release/anvil --version
```

### Add to PATH (optional)

```bash
# Add to your shell profile
export PATH="$PATH:/path/to/anvil-lang/target/release"
```

## Your First Verification

### 1. Check the Hello World example

```bash
anvil check examples/hello.anv
```

Expected output:

```
  ╔═══════════════════════════════════════════╗
  ║   ▄▀█ ███▄ █ █ █ █ █   █                 ║
  ║   █▀█ █  ▀██ ▀▄▀ █ █▄▄ █▄▄    v0.5      ║
  ║   Where trust doesn't compile.            ║
  ╚═══════════════════════════════════════════╝

  → Parsing examples/hello.anv...
  ✓ Parsed: 1 functions, 3 invariants
  ✓ 2 type constraints registered for Z3
  → Verifying with Z3...

╔══════════════════════════════════════════════════╗
║         ANVIL VERIFICATION REPORT                ║
╚══════════════════════════════════════════════════╝

  ✓ add — 2 preconditions, 1 postconditions verified (3 invariants) in X.XXXms
    🔐 proof: abcdef0123456789…12345678

  █ All 1/1 postconditions proven. Zero trust required.
```

### 2. Understand the output

- **✓ PROVEN** means Z3 mathematically proved the invariants are satisfied
- **✗ REJECTED** means Z3 found a counterexample — your code has a bug
- **🔐 proof hash** is a SHA3-256 cryptographic anchor of the Z3 assertion set

### 3. Try a failing example

Create `bad.anv`:

```anvil
fn broken_transfer(mut balance: u64, amount: u64) -> u64
    where {
        amount > 0,
        balance >= amount,
        balance' == balance + amount  // BUG: should be balance - amount
    }
{
    balance -= amount;
    return balance;
}
```

```bash
anvil check bad.anv
```

Z3 will find a counterexample proving the postcondition is violated.

## Writing Invariants

### Preconditions (input constraints)

```anvil
fn safe_div(a: u64, b: u64) -> u64
    where {
        b > 0,           // Precondition: no division by zero
        a' == a / b      // Postcondition: result is the quotient
    }
{ ... }
```

### Postconditions (output guarantees)

Use the `'` (prime) notation to refer to the post-state value:

- `a` = value of `a` before the function
- `a'` = value of `a` after the function

```anvil
// Conservation law: total supply is preserved
sender_balance' + receiver_balance' == sender_balance + receiver_balance
```

### Contract-level invariants

```anvil
contract Token {
    state {
        total_supply: u64,
        balance_a: u64,
        balance_b: u64,
    }

    invariant {
        balance_a + balance_b == total_supply
    }

    fn transfer(...) { ... }
}
```

## CLI Commands

| Command | Description |
|---|---|
| `anvil check <file>` | Parse, type-check, and verify |
| `anvil build <file> -o <out>` | Compile to Rust (only if verified) |
| `anvil build <file> -o <out> -t llvm` | Compile to LLVM IR |
| `anvil ast <file>` | Dump AST as JSON |
| `anvil lsp` | Start Language Server Protocol |
| `anvil saas --port 3000` | Start Proof Market SaaS API |

## Structured Logging

Anvil uses `tracing` for structured logging. Control verbosity with `RUST_LOG`:

```bash
# Default (info level)
anvil check examples/hello.anv

# Debug level (shows type constraints, solver details)
RUST_LOG=debug anvil check examples/hello.anv

# Trace level (maximum detail)
RUST_LOG=trace anvil check examples/hello.anv
```

## Next Steps

- Browse [examples/](../examples/) for more verification patterns
- Read the [SaaS Guide](saas-guide.md) to run the Proof Market API
- See [CONTRIBUTING.md](../CONTRIBUTING.md) to contribute
