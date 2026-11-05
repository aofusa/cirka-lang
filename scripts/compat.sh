#!/usr/bin/env bash
# Compatibility measurement against GHC's testsuite / nofib (FEAT-0119).
# Classifies failures; the pass rate is not a goal (ADR-025).
#
# The GHC testsuite programs are committed under tests/achc/compat/ghc
# (re-import with scripts/import-compat.py). nofib is not redistributed: set
# CIRKA_NOFIB_DIR to a nofib checkout to import it into _build/compat/nofib.
# Results: docs/artifacts/compat/{results,summary}.tsv. CIRKA_BLESS=1 rewrites
# tests/achc/compat/baseline.tsv; CIRKA_COMPAT_FILTER=<substring> limits the
# programs; CIRKA_COMPAT_TIMEOUT=<seconds> is the run time limit.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
if [ -n "${CIRKA_NOFIB_DIR:-}" ] && [ ! -d _build/compat/nofib ]; then
  python3 scripts/import-compat.py --nofib "$CIRKA_NOFIB_DIR"
fi
if [ ! -d _build/compat/nofib ]; then
  echo "compat: CIRKA_NOFIB_DIR is empty; measuring the GHC testsuite programs only"
fi
scripts/build-runtime.sh
cargo test -q --test achc_compat -- --ignored --nocapture achc_compat "$@"
