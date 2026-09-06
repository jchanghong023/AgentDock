#!/usr/bin/env bash
# Linux only. The temporary state never points at the user's normal history.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
command -v xvfb-run >/dev/null || { echo 'BLOCKED: xvfb-run not installed' >&2; exit 2; }
command -v cargo >/dev/null || { echo 'BLOCKED: cargo not installed' >&2; exit 2; }
cargo build --locked
state=$(mktemp -d)
trap 'rm -rf -- "$state"' EXIT
xvfb-run -a -s '-screen 0 1240x820x24' timeout 30s \
    target/debug/agentdock --state-dir "$state" --smoke-ui-ms 2000 --open examples/README.md \
    >"$state/stdout.log" 2>"$state/stderr.log"
cat "$state/stdout.log" "$state/stderr.log"
grep -q 'AGENTDOCK_GUI_SMOKE_EVENT_LOOP_OK' "$state/stderr.log"
echo 'GUI event loop and automatic close completed. This is not an IME or visual quality test.'
