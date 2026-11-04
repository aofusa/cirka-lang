#!/usr/bin/env bash
# Builds libcirka_rt.a in both flavours:
#   target/rt/release/libcirka_rt.a        (release runtime)
#   target/rt-debug/release/libcirka_rt.a  (debug runtime: checks + leak report)
# The compiler drivers look for these unless CIRKA_RUNTIME_DIR is set.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p cirka-runtime --release --target-dir target/rt
cargo build -q -p cirka-runtime --release --features debug-rt --target-dir target/rt-debug
