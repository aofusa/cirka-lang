# aCHC development / build environment (x86_64 Linux, Ubuntu 24.04).
#
#   docker build -t achc-dev .
#   docker run --rm -it -v "$PWD":/work achc-dev
#
# or simply: docker compose run --rm dev
FROM ubuntu:24.04

ARG RUST_TOOLCHAIN=1.99.1
ARG USERNAME=dev
ARG USER_UID=1000
ARG USER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# - build-essential               : C toolchain (LLVM comes last, see below)
# - ghc / libgmp-dev              : reference implementation for differential tests
# - z3                            : SMT solver for `achc verify` (refinement types)
# - valgrind / hyperfine / time   : memory-safety checks, benchmarks, peak RSS
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git pkg-config python3 sudo \
        ghc libgmp-dev \
        z3 \
        valgrind hyperfine time \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain (pinned; keep in sync with rust-toolchain.toml)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
            --default-toolchain ${RUST_TOOLCHAIN} -c rustfmt -c clippy \
    && chmod -R a+rwX ${RUSTUP_HOME} ${CARGO_HOME}

# Non-root user whose uid/gid match the host user (Ubuntu 24.04 ships a
# default "ubuntu" user with uid 1000, which we remove first).
RUN (userdel -r ubuntu 2>/dev/null || true) \
    && groupadd -g ${USER_GID} ${USERNAME} \
    && useradd -m -u ${USER_UID} -g ${USER_GID} -s /bin/bash ${USERNAME} \
    && echo "${USERNAME} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/${USERNAME} \
    && mkdir -p /work/target ${CARGO_HOME}/registry \
    && chown ${USER_UID}:${USER_GID} /work/target ${CARGO_HOME}/registry
# (Named volumes mounted on /work/target and the cargo registry inherit the
#  ownership above, so the non-root user can write to them.)

# Vulkan headers and loader: `bindgen` reads vulkan_core.h in its tests
# (FEAT-0048). A separate layer, so that adding it keeps the toolchain
# layers above cached.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libvulkan-dev \
    && rm -rf /var/lib/apt/lists/*

# zlib headers and the link library: the C library that CirkaLang's FFI
# sample wraps (FEAT-0250, spec-v0.0.6.md §1.8).
RUN apt-get update && apt-get install -y --no-install-recommends \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# Vulkan compute without a GPU (FEAT-0251, docs/design/15-gpu-and-remote.md
# §2): Mesa's lavapipe (a Vulkan driver running on the CPU), the validation
# layers, vulkaninfo, and glslc / spirv-tools for GLSL compute shaders.
RUN apt-get update && apt-get install -y --no-install-recommends \
        mesa-vulkan-drivers vulkan-tools vulkan-validationlayers glslc spirv-tools \
    && rm -rf /var/lib/apt/lists/*

# Linear algebra (FEAT-0252, docs/design/09-libraries.md §1.5): OpenBLAS
# (CBLAS) and LAPACKE, wrapped by the CirkaLang library `Linalg`.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libopenblas-dev liblapacke-dev \
    && rm -rf /var/lib/apt/lists/*

# LLVM, from apt.llvm.org: the same major version as the LLVM of the pinned
# Rust toolchain (ADR-110, FEAT-1000; Ubuntu 24.04's own packages stop at an
# older one). The last layer, so that changing LLVM_VERSION rebuilds only it.
# - clang / lld             : compile the generated LLVM IR and link it with
#                             libcirka_rt
# - llvm-*-dev / libclang-* : LLVM tools, and libclang for `bindgen`
# - libclang-rt-*-dev       : sanitizer runtimes (ThreadSanitizer for the
#                             concurrency tests, FEAT-0045)
ARG LLVM_VERSION=23
RUN install -d -m 0755 /etc/apt/keyrings \
    && curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key -o /etc/apt/keyrings/apt.llvm.org.asc \
    && echo "deb [signed-by=/etc/apt/keyrings/apt.llvm.org.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-${LLVM_VERSION} main" \
        > /etc/apt/sources.list.d/apt.llvm.org.list \
    && apt-get update && apt-get install -y --no-install-recommends \
        clang-${LLVM_VERSION} llvm-${LLVM_VERSION} llvm-${LLVM_VERSION}-dev \
        lld-${LLVM_VERSION} libclang-${LLVM_VERSION}-dev libclang-rt-${LLVM_VERSION}-dev \
    && rm -rf /var/lib/apt/lists/*
ENV PATH=/usr/lib/llvm-${LLVM_VERSION}/bin:$PATH

USER ${USERNAME}
WORKDIR /work
CMD ["bash"]
