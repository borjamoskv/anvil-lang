# Contributing to Anvil

Thank you for your interest in contributing to Anvil — a formally verified programming language where trust doesn't compile.

## Development Setup

### Prerequisites

1. **Rust** (stable, ≥ 1.85)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default stable
   ```

2. **Z3 SMT Solver** (≥ 4.8)
   - **Ubuntu/Debian:** `sudo apt-get install -y z3 libz3-dev`
   - **macOS:** `brew install z3`
   - **Windows:** `choco install z3`
   - See [docs/z3-installation.md](docs/z3-installation.md) for detailed instructions.

3. **Clang/LLVM** (for LLVM backend)
   ```bash
   # Ubuntu
   sudo apt-get install -y clang llvm
   # macOS
   brew install llvm
   ```

### Building

```bash
git clone https://github.com/borjamoskv/anvil-lang.git
cd anvil-lang
cargo build
```

### Running Tests

```bash
# Unit tests
cargo test --bin anvil -- --test-threads=1

# Verify example programs
cargo run -- check examples/hello.anv
cargo run -- check examples/transfer.anv
```

### CI Profiles

Run the lightweight gate before pushing:

```bash
scripts/ci-quick.sh
```

This runs format, compile checks, root unit tests, and Proof Market unit tests.

Run the full gate when touching proof, integration, Proof Market, or release paths:

```bash
scripts/ci-full.sh
```

This runs integration tests, Python Proof Market checks, hello/transfer example verification, and release builds.

If branch protection requires named checks, use `Quick Checks`, `Full Checks`, and `Security Audit`.

### Cache Hygiene

This repo may use a shared Cargo target directory. Prefer package-scoped cleanup over deleting the whole target cache:

```bash
scripts/clean-builds.sh --dry-run
scripts/clean-builds.sh
```

## Code Style

### Formatting

All code must pass `rustfmt`:

```bash
cargo fmt -- --check  # Verify
cargo fmt             # Auto-fix
```

### Static Checks

All code must pass the lightweight compile check:

```bash
cargo check --bin anvil
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(verifier): add support for array bounds in Z3
fix(parser): handle nested contract definitions
docs: update Z3 installation guide for Windows
test: add vault deposit verification test
ci: add MSRV matrix build
```

**Types:** `feat`, `fix`, `docs`, `test`, `ci`, `refactor`, `perf`, `chore`

**Scopes:** `parser`, `typechecker`, `verifier`, `codegen`, `lsp`, `saas`, `cli`

## Pull Request Workflow

1. Fork the repository
2. Create a feature branch from `main`: `git checkout -b feat/my-feature`
3. Make your changes
4. Ensure CI passes locally:
   ```bash
   scripts/ci-quick.sh
   ```
   Run the full gate above when the PR touches proof, integration, Proof Market, or release paths.
5. Push and open a Pull Request against `main`
6. Fill in the PR template

### PR Requirements

- [ ] `cargo fmt` passes
- [ ] Quick checks pass
- [ ] Full checks run when touching proof, integration, Proof Market, or release paths
- [ ] Build-cache cleanup, if needed, used `scripts/clean-builds.sh`
- [ ] Documentation updated (if applicable)
- [ ] Breaking changes documented

## Adding Examples

Anvil examples live in `examples/`. To add a new example:

1. Create `examples/your_example.anv`
2. Add comments explaining what the example demonstrates
3. Include both **passing** and **failing** invariants where appropriate
4. Add a row to the Examples table in `README.md`
5. Verify it locally with `cargo run --bin anvil -- check examples/your_example.anv` and include the command in the PR testing notes.

### Example template:

```anvil
// ============================================================
// EXAMPLE: [Title]
// [Description of what this example demonstrates]
// ============================================================

fn your_function(param: u64) -> u64
    where {
        // Preconditions
        param > 0,
        // Postconditions
        param' == param + 1
    }
{
    param += 1;
    return param;
}
```

## Architecture

```
src/
├── main.rs          # CLI entry point
├── grammar.pest     # PEG grammar definition
├── ast.rs           # Abstract Syntax Tree types
├── parser.rs        # Pest parser → AST
├── typechecker.rs   # Bidirectional type inference
├── verifier.rs      # Z3 SMT solver integration
├── codegen.rs       # Rust code generation
├── llvm_ir.rs       # LLVM IR code generation
├── lsp.rs           # Language Server Protocol
└── saas.rs          # Proof Market SaaS API
```

## Verification Pipeline

```
.anv source → Parser → AST → Type Checker → Z3 Verifier → Codegen → .rs
```

Each stage must preserve invariants from the previous stage. When modifying any pipeline stage, ensure downstream stages are not affected.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
