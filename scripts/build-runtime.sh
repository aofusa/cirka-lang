#!/usr/bin/env bash
# Builds libcirka_rt.a in its flavours:
#   target/rt/release/libcirka_rt.a        (release runtime)
#   target/rt-debug/release/libcirka_rt.a  (debug runtime: checks + leak report)
#   target/rt-tsan/release/libcirka_rt.a   (ThreadSanitizer, --runtime=tsan)
# The compiler drivers look for these unless CIRKA_RUNTIME_DIR is set.
# Stable Rust has no sanitizers: the tsan flavour is built with
# RUSTC_BOOTSTRAP=1; `core` is not instrumented (05-runtime.md §9.7). It
# calls libc's I/O functions (`--cfg cirka_tsan`, FEAT-1007).
# The runtime is only linked into executables: it is compiled as their
# position-independent code (`-C relocation-model=pie`), so that its calls
# and accesses to its own functions and statics do not go through the GOT
# (FEAT-1026; the same flags as crates/cirka-driver/src/native.rs).
set -euo pipefail
cd "$(dirname "$0")/.."
pie="-C relocation-model=pie"
RUSTFLAGS="$pie" cargo build -q -p cirka-runtime --release --target-dir target/rt
RUSTFLAGS="$pie" cargo build -q -p cirka-runtime --release --features debug-rt --target-dir target/rt-debug
RUSTC_BOOTSTRAP=1 RUSTFLAGS="$pie -Zsanitizer=thread -Cunsafe-allow-abi-mismatch=sanitizer --cfg cirka_tsan" \
    cargo build -q -p cirka-runtime --release --target-dir target/rt-tsan
