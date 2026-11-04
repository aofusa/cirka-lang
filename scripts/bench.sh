#!/usr/bin/env bash
# Benchmarks (10-testing.md §5). Builds every implementation found under
# bench/<name>/ and compares them with hyperfine.
#   scripts/bench.sh [name...]
# Raw results go to docs/artifacts/bench/ (git-ignored).
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
exec bench/run.sh "$@"
