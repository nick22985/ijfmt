#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

for script in install scripts/*.sh scripts/fixtures/*.sh; do
    bash -n "$script"
done
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --lib --bins --quiet
cargo test --locked --release --all-targets -- --quiet
cargo test --locked --doc --quiet
cargo test --locked --release --test java_semantics_test -- --ignored
