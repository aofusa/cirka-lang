#!/usr/bin/env bash
# Compatibility measurement against GHC's testsuite / nofib (FEAT-0119).
# Classifies failures; the pass rate is not a goal (ADR-025).
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
if [ -z "${CIRKA_GHC_TESTSUITE_DIR:-}" ]; then
  echo "compat: CIRKA_GHC_TESTSUITE_DIR is empty; measuring tests/achc/compat only"
fi
scripts/build-runtime.sh
cargo test -q --test achc_compat -- --nocapture "$@"
