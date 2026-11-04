#!/usr/bin/env bash
# CI entry point (10-testing.md §6). Run inside the Docker container:
#   docker compose run --rm dev scripts/ci.sh
# Every stage must succeed; the script exits non-zero on the first failure.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh

stage() { echo; echo "==> $*"; }

stage "cargo fmt --check"
cargo fmt --all -- --check

stage "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

stage "build runtime (release + debug)"
scripts/build-runtime.sh

stage "cargo test"
cargo test --workspace

echo
echo "ci: all stages passed"
