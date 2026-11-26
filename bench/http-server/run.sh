#!/usr/bin/env bash
# The HTTP server benchmark (FEAT-0243, 10-testing.md §5). The CirkaLang
# server (server.cirka) and the Rust one (rust/server.rs: one thread and
# epoll, like the event loop of the CirkaLang runtime) answer `GET /` with
# `Hello, World!`; load.rs sends keep-alive requests, one at a time on
# each connection, and reports the throughput and the latency.
#
#   CIRKA_HTTP_CONNS     numbers of connections (default "1 16 64")
#   CIRKA_HTTP_REQUESTS  requests of a run, split among the connections
#                        (default 200000)
#   CIRKA_BENCH_RUNS     rounds of each number; a round runs both servers
#                        (default 3)
#
# The servers alternate within each round and take turns going first, so
# that a change of the machine's load during the measurement falls on both
# (FEAT-0086). Results go to docs/artifacts/bench/http-server.md
# (git-ignored): the median of the rounds, the peak RSS of the server
# (VmHWM), and the median of the ratio Rust / CirkaLang of the requests per
# second of each round (above 1: Rust faster).
# Usage: bench/http-server/run.sh (in the Docker image).
set -euo pipefail
cd "$(dirname "$0")/../.."
[ -n "${CIRKA_ROOT:-}" ] || source scripts/env.sh
out=docs/artifacts/bench
bin=_build/bench
mkdir -p "$out" "$bin"
cargo build -q --release -p cirka
scripts/build-runtime.sh
target/release/cirka build -O2 -o "$bin/http-cirka" bench/http-server/server.cirka
rustc -C opt-level=3 -o "$bin/http-rust" bench/http-server/rust/server.rs
rustc -C opt-level=3 -o "$bin/http-load" bench/http-server/load.rs
runs="${CIRKA_BENCH_RUNS:-3}"
conns_list="${CIRKA_HTTP_CONNS:-1 16 64}"
total="${CIRKA_HTTP_REQUESTS:-200000}"
tmp=$(mktemp -d)
pid=
trap '[ -n "$pid" ] && kill "$pid" 2>/dev/null; rm -rf "$tmp"' EXIT
raw="$tmp/raw"
for conns in $conns_list; do
  for ((r = 1; r <= runs; r++)); do
    if ((r % 2)); then order="cirka rust"; else order="rust cirka"; fi
    for impl in $order; do
      portf="$tmp/port-$impl-$conns-$r"
      "$bin/http-$impl" 0 > "$portf" &
      pid=$!
      for _ in $(seq 200); do [ -s "$portf" ] && break; sleep 0.05; done
      port=$(head -1 "$portf")
      res=$("$bin/http-load" "$port" "$conns" $((total / conns)))
      rss=$(awk '/^VmHWM/ {print $2}' "/proc/$pid/status")
      kill "$pid"
      wait "$pid" 2>/dev/null || true
      pid=
      echo "round=$r impl=$impl conns=$conns $res rss_kib=$rss" | tee -a "$raw"
    done
  done
done
python3 - "$raw" > "$out/http-server.md" <<'PY'
import sys, collections, statistics
rows = collections.defaultdict(dict)
for line in open(sys.argv[1]):
    kv = dict(f.split("=", 1) for f in line.split())
    rows[(kv["impl"], int(kv["conns"]))][int(kv["round"])] = kv
print("| server | connections | requests/s | p50 (µs) | p90 (µs) | p99 (µs) | peak RSS (KiB) |")
print("|---|---:|---:|---:|---:|---:|---:|")
for impl, conns in sorted(rows, key=lambda k: (k[0] != "cirka", k[1])):
    runs = rows[(impl, conns)].values()
    med = lambda k: statistics.median(float(r[k]) for r in runs)
    print(f"| {impl} | {conns} | {med('rps'):.0f} | {med('p50_us'):.1f} | {med('p90_us'):.1f} "
          f"| {med('p99_us'):.1f} | {max(int(r['rss_kib']) for r in runs)} |")
print()
print("| connections | Rust / CirkaLang (requests/s, median of the rounds) |")
print("|---:|---:|")
for conns in sorted({c for _, c in rows}):
    c, r = rows[("cirka", conns)], rows[("rust", conns)]
    ratio = statistics.median(float(r[k]["rps"]) / float(c[k]["rps"]) for k in c if k in r)
    print(f"| {conns} | {ratio:.3f} |")
PY
cat "$out/http-server.md"
