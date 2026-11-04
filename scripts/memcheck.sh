#!/usr/bin/env bash
# Runs the native execution tests under valgrind with the system allocator
# (CIRKA_ALLOC=system). Slow; required at milestone completion and releases.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
command -v valgrind >/dev/null || { echo "memcheck: valgrind not found" >&2; exit 1; }
scripts/build-runtime.sh
export CIRKA_ALLOC=system
export CIRKA_VALGRIND=1
cargo test --workspace --test '*' -- --nocapture "$@"
