#!/usr/bin/env bash
# Requires an installed Rust toolchain and downloadable/cached dependencies.
# Formats repository source files before compiling. Does not modify user data.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
    printf '%s\n' 'BLOCKED: cargo not installed; no Rust verification was performed.' >&2
    exit 2
fi
cargo --version
rustc --version
cargo fmt --all
if [[ ! -f Cargo.lock ]]; then
    cargo generate-lockfile
fi
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets
cargo run --locked -- --self-test
printf '%s\n' 'Rust build/check/test/PTY checks completed on this host. Other platforms remain unverified.'
