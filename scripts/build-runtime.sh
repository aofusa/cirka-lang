#!/usr/bin/env bash
# Builds libcirka_rt.a in its flavours:
#   target/rt/release/libcirka_rt.a        (release runtime)
#   target/rt-debug/release/libcirka_rt.a  (debug runtime: checks + leak report)
#   target/rt-tsan/release/libcirka_rt.a   (ThreadSanitizer, --runtime=tsan)
# The compiler drivers look for these unless CIRKA_RUNTIME_DIR is set.
# Stable Rust has no sanitizers: the tsan flavour is built with
# RUSTC_BOOTSTRAP=1; `core` is not instrumented (05-runtime.md §9.7).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p cirka-runtime --release --target-dir target/rt
cargo build -q -p cirka-runtime --release --features debug-rt --target-dir target/rt-debug
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zsanitizer=thread -Cunsafe-allow-abi-mismatch=sanitizer" \
    cargo build -q -p cirka-runtime --release --target-dir target/rt-tsan
