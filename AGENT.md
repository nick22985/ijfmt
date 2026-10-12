# Agent Instructions

## Critical Rules

### NEVER modify fixture files
- **NEVER** run the formatter binary (`ijfmt`, `cargo run`, `./target/debug/ijfmt`) on any path inside `tests/fixtures/`
- **NEVER** edit, overwrite, or delete any file under `tests/fixtures/`
- Fixtures are the ground truth. They must never change.
- If you need to test formatter output on a fixture input, copy the file to `/tmp/` first, then run the formatter on the copy.

## Project Overview

Rust-based IntelliJ XML formatter (`ijfmt`) that formats Java source files exactly the way IntelliJ IDEA does.

## Source Layout

- `src/lib.rs` — shared library used by the CLI, tests, and benchmarks
- `src/config/` — IntelliJ XML configuration parsing
- `src/formatter/` — formatter pipeline: `parse`, `build`, `flatten`, `wrap`, `emit`
- `src/formatter/build/` — Java block builder, grouped into `declarations/`, `expressions/`, `statements/` and `comments/`
- `src/main.rs` — process entrypoint, calls `cli::run`
- `src/cli/` — binary-only code: `args.rs` (command-line arguments), `style.rs` (style discovery), `files/` (file collection, git changed lines, parallel formatting, atomic writes)
- Modules with submodules use `foo/mod.rs`, not `foo.rs` beside `foo/`

## Running Checks

- Unit tests: `cargo test --lib --bins --quiet`
- Fixture test: `cargo test --test integration_test --quiet -- <fixture-name>`
- Full fixture corpus: `cargo test --release --test integration_test --quiet`
- Warnings: `cargo clippy --all-targets -- -D warnings`

## Fixture Testing

The integration harness loads style XML from `tests/fmt/` and compares formatter output with the matching `.expected.java` fixture. Fix mismatches in source code only.
