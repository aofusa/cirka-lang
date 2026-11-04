#!/usr/bin/env bash
# Builds and compares every implementation found in bench/<name>/ with
# hyperfine (10-testing.md §5). Usage: bench/run.sh [name...]
#
#   mir/main.mir   hand-written MIR            (cirka-mir build)
#   main.cirka     CirkaLang                   (cirka build)
#   own.chs        aCHC with ownership         (achc build)
#   plain.hs       plain Haskell: aCHC and GHC -O2
#   rust/main.rs   Rust baseline               (rustc -C opt-level=3)
#
# Program arguments come from bench/<name>/args. Results (markdown/JSON)
# go to docs/artifacts/bench/ (git-ignored).
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${CIRKA_ROOT:-}" ] || source scripts/env.sh
out=docs/artifacts/bench
bin=_build/bench
mkdir -p "$out" "$bin"
cargo build -q --release -p cirka-driver -p achc -p cirka
scripts/build-runtime.sh
runs="${CIRKA_BENCH_RUNS:-5}"
names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  for d in bench/*/; do names+=("$(basename "$d")"); done
fi
for name in "${names[@]}"; do
  dir="bench/$name"
  [ -d "$dir" ] || continue
  args="$(cat "$dir/args" 2>/dev/null || true)"
  cmds=()
  if [ -f "$dir/mir/main.mir" ]; then
    target/release/cirka-mir build -O2 -o "$bin/$name-mir" "$dir/mir/main.mir"
    cmds+=(-n mir "$bin/$name-mir $args")
  fi
  if [ -f "$dir/main.cirka" ] && target/release/cirka build -O2 -o "$bin/$name-cirka" "$dir/main.cirka" 2>/dev/null; then
    cmds+=(-n cirka "$bin/$name-cirka $args")
  fi
  if [ -f "$dir/own.chs" ] && target/release/achc build -O2 -o "$bin/$name-chs" "$dir/own.chs" 2>/dev/null; then
    cmds+=(-n achc-own "$bin/$name-chs $args")
  fi
  if [ -f "$dir/plain.hs" ]; then
    if target/release/achc build -O2 -o "$bin/$name-achc" "$dir/plain.hs" 2>/dev/null; then
      cmds+=(-n achc-plain "$bin/$name-achc $args")
    fi
    if [ -n "${CIRKA_GHC:-}" ] && command -v "$CIRKA_GHC" >/dev/null; then
      "$CIRKA_GHC" -O2 -v0 -outputdir "$bin/ghc-$name" -o "$bin/$name-ghc" "$dir/plain.hs"
      cmds+=(-n ghc-O2 "$bin/$name-ghc $args")
    fi
  fi
  if [ -f "$dir/rust/main.rs" ]; then
    rustc -C opt-level=3 -o "$bin/$name-rust" "$dir/rust/main.rs"
    cmds+=(-n rust "$bin/$name-rust $args")
  fi
  echo "== $name"
  hyperfine -N -w 1 -r "$runs" --export-markdown "$out/$name.md" --export-json "$out/$name.json" "${cmds[@]}"
  # peak RSS of each implementation (needs GNU time)
  [ -x /usr/bin/time ] || continue
  for ((i = 1; i < ${#cmds[@]}; i += 3)); do
    label="${cmds[$i]}"; cmd="${cmds[$((i + 1))]}"
    rss=$( { /usr/bin/time -f '%M' $cmd >/dev/null; } 2>&1 | tail -1 )
    echo "max RSS $label: ${rss} KiB" | tee -a "$out/$name.md"
  done
done
