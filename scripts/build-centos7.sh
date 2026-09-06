#!/usr/bin/env bash
# Execute only in a provisioned glibc-2.17 build root with a compatible modern
# Rust/C toolchain and X11 development libraries. Does not install packages.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
actual=$(getconf GNU_LIBC_VERSION)
if [[ "$actual" != 'glibc 2.17' ]]; then
    printf 'Refusing to label a build as CentOS 7: host is %s, expected glibc 2.17.\n' "$actual" >&2
    exit 2
fi
command -v cargo >/dev/null || { echo 'BLOCKED: cargo not installed' >&2; exit 2; }
command -v readelf >/dev/null || { echo 'BLOCKED: readelf not installed' >&2; exit 2; }
[[ -f Cargo.lock ]] || cargo generate-lockfile
cargo build --release --locked --target x86_64-unknown-linux-gnu
readelf --version-info --wide target/x86_64-unknown-linux-gnu/release/agentdock
printf '%s\n' 'Build finished. Audit GLIBC/GLIBCXX and every shipped .so, then test on a real CentOS 7 host.'
