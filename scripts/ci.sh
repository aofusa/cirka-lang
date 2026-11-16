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

stage "Vulkan headers"
# The bindgen tests read vulkan_core.h (FEAT-0048) and skip without it;
# CI requires it (libvulkan-dev in the Dockerfile).
if printf '#include <vulkan/vulkan_core.h>\n' \
    | "${CIRKA_CLANG:-clang}" -fsyntax-only -x c - 2>/dev/null; then
    echo "ci: vulkan_core.h is installed: the Vulkan bindgen tests run"
else
    echo "ci: ERROR: vulkan/vulkan_core.h is not installed; rebuild the image"
    echo "ci:        (docker compose build) to get libvulkan-dev"
    exit 1
fi

stage "Vulkan compute"
# The tests with `requires: vulkan` (spec-v0.0.6.md §3, FEAT-0251) compile
# GLSL shaders with glslc and run them on a Vulkan device with the
# validation layer, and skip when either is missing; CI requires them (Mesa
# lavapipe, the validation layers and glslc in the Dockerfile).
# (The summary is read whole: `grep -q` in a pipe could end vulkaninfo
# with SIGPIPE, which pipefail reports.)
summary=$(vulkaninfo --summary 2>/dev/null) || summary=""
if ! "${CIRKA_GLSLC:-glslc}" --version >/dev/null 2>&1; then
    echo "ci: ERROR: glslc (CIRKA_GLSLC) is not installed; rebuild the image"
    exit 1
elif [ -z "$summary" ]; then
    echo "ci: ERROR: no Vulkan device (vulkaninfo --summary); rebuild the image"
    echo "ci:        to get Mesa lavapipe (mesa-vulkan-drivers)"
    exit 1
elif ! grep -q VK_LAYER_KHRONOS_validation <<<"$summary"; then
    echo "ci: ERROR: the validation layer VK_LAYER_KHRONOS_validation is not installed"
    exit 1
else
    echo "ci: a Vulkan device, the validation layer and glslc are installed: the Vulkan tests run"
fi

stage "cargo test"
cargo test --workspace

stage "runtime unit tests (optimized)"
# Programs link the optimized runtime. Inlining changes stack layouts, so a
# C call overrunning a buffer may only show up here (BUG-0025).
cargo test --release -p cirka-runtime --lib

echo
echo "ci: all stages passed"
