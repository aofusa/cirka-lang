#!/usr/bin/env bash
# Runs the native execution tests under valgrind with the system allocator
# (CIRKA_ALLOC=system): CIRKA_VALGRIND=1 makes the test harness run each
# native program (except ThreadSanitizer builds) through valgrind's memcheck
# (CIRKA_VALGRIND_BIN) and fail the case on any error (BUG-0038). Slow;
# required at milestone completion and releases. Extra arguments go to the
# test binaries (e.g. a test name).
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
command -v "${CIRKA_VALGRIND_BIN:-valgrind}" >/dev/null || { echo "memcheck: valgrind not found" >&2; exit 1; }
scripts/build-runtime.sh
export CIRKA_ALLOC=system
export CIRKA_VALGRIND=1
cargo test --workspace --test '*' -- --nocapture "$@"
