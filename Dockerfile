# aCHC development / build environment (x86_64 Linux, Ubuntu 24.04).
#
#   docker build -t achc-dev .
#   docker run --rm -it -v "$PWD":/work achc-dev
#
# or simply: docker compose run --rm dev
FROM ubuntu:24.04

ARG LLVM_VERSION=18
ARG RUST_TOOLCHAIN=1.99.1
ARG USERNAME=dev
ARG USER_UID=1000
ARG USER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/lib/llvm-${LLVM_VERSION}/bin:$PATH

# - build-essential / clang / lld : link generated objects with libcirka_rt
# - llvm-*-dev / libclang-*-dev   : LLVM tools, and libclang for `achc bindgen`
# - libclang-rt-*-dev             : sanitizer runtimes (ThreadSanitizer for the
#                                   concurrency tests, FEAT-0045)
# - ghc / libgmp-dev              : reference implementation for differential tests
# - z3                            : SMT solver for `achc verify` (refinement types)
# - valgrind / hyperfine / time   : memory-safety checks, benchmarks, peak RSS
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git pkg-config python3 sudo \
        clang-${LLVM_VERSION} llvm-${LLVM_VERSION} llvm-${LLVM_VERSION}-dev \
        lld-${LLVM_VERSION} libclang-${LLVM_VERSION}-dev libclang-rt-${LLVM_VERSION}-dev \
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

USER ${USERNAME}
WORKDIR /work
CMD ["bash"]
