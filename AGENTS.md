# AGENTS.md — CORTEX C5-REAL Standard

## Project Context
<!-- JULES: This repo is part of the CORTEX ecosystem by Borja Moskv (borjamoskv). -->
<!-- Customize this section per repo if needed. -->

## Setup Commands
- Python: `pip install -e ".[dev]"` or `pip install -r requirements.txt`
- Node: `npm ci`
- Rust: `cargo build`

## Build & Test
- Python: `pytest -x --tb=short`
- Node: `npm test`
- Rust: `cargo test`

## Code Style (Invariants)
- Zero defensive programming. Fail-fast: crash over catch.
- Conventional Commits mandatory: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`.
- Zero noise in comments. No `# TODO: maybe`, no `# This might help`, no `# placeholder`.
- Type hints strict (Python). `strict: true` (TypeScript). Explicit types over `any`.
- Docstrings only when they add causal context, not obvious descriptions.
- No decorative prose in code. Every comment must justify a non-obvious decision.

## Agent Constraints
- NEVER introduce generic `try/except Exception` or `catch(e)` blocks.
- NEVER add placeholder comments like "Add more tests here" or "TODO: implement".
- NEVER reorganize project structure without explicit instruction.
- ALWAYS run the full test suite before submitting a PR.
- ALWAYS preserve existing comments and docstrings unrelated to your changes.
- ALWAYS use the existing project structure and conventions.
- Commit messages MUST follow Conventional Commits specification.
- PR descriptions MUST include a concise summary of what changed and why.

## Testing Requirements
- Every bug fix PR must include a regression test.
- New functions must have at least one happy-path test.
- Test names must describe the behavior being tested, not the function name.

## Author
All generated code credits: Borja Moskv (borjamoskv)
