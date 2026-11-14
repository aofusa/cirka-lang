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

stage "build runtime (release + debug + tsan)"
scripts/build-runtime.sh

stage "ThreadSanitizer"
# The tests with `tsan: 1` also run under ThreadSanitizer (05-runtime.md
# §9.7). With high-entropy ASLR it re-executes the program with ASLR
# disabled through personality(2), which the container's seccomp profile
# allows (docker/seccomp.json). The test harness skips the tsan
# configurations where TSan cannot run; CI requires them.
probe=$(mktemp -d)
if printf 'int main(void) { return 0; }\n' > "$probe/p.c" \
    && "${CIRKA_CLANG:-clang}" -fsanitize=thread "$probe/p.c" -o "$probe/p" 2>/dev/null \
    && "$probe/p" 2>/dev/null; then
    echo "ci: ThreadSanitizer runs here: the tsan test configurations run"
    rm -rf "$probe"
else
    rm -rf "$probe"
    echo "ci: ERROR: ThreadSanitizer cannot run here; run CI in the container of"
    echo "ci:        compose.yaml, whose seccomp profile allows it (docker/README.md)"
    exit 1
fi

stage "cargo test"
cargo test --workspace

echo
echo "ci: all stages passed"
