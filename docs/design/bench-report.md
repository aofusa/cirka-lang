# 性能計測レポート

ベンチマークの結果を記録する (計測方法は [10-testing.md](10-testing.md) §5)。
生データ (hyperfine の Markdown / JSON) は `docs/artifacts/bench/` に出力され、git 管理外。

計測: `scripts/bench.sh <name>` (Docker コンテナ内、`CIRKA_BENCH_RUNS` 回の平均)。

## 計測環境

- CPU: Intel Core i5-3320M (2 コア 4 スレッド, 2.6 GHz)、x86_64 Linux、Docker (Ubuntu 24.04)
- LLVM 18 (clang `-O2`)、rustc (rust-toolchain.toml の版, `-C opt-level=3`)

## M1 ベースライン (FEAT-0016)

### binary-trees (深さ 21, 単一スレッド)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| 手書き MIR + 本ランタイム (`cirka-mir build -O2`) | 15.59 s ± 0.06 | 0.67 |
| Rust (`Option<Box<Node>>`, glibc malloc) | 23.10 s ± 0.01 | 1.00 |

- 受け入れ条件「Rust の 2 倍以内」を満たす (MIR の方が 1.48 倍速い)。
- MIR 版が速い主因はアロケータ: 本ランタイムはサイズクラス別のフリーリスト (05-runtime.md §3) で、
  glibc malloc/free より割り当て・解放が軽い。RC の dup/drop はインライン展開され、
  木の構築 (所有権の移動のみ) では RC 操作がほぼ発生しない。
- 葉の `Nil` は静的オブジェクト (rc=0) を共有するため割り当てない。Rust 版も `None` で割り当てない (公平な比較)。
  最初の Rust 版は `Nil` ごとに `Box` を割り当てており 37.9 s だった。
- 参考: Rust 側を mimalloc 等に替えると差は縮まる見込み。本計測の目的は
  「RC + 本ランタイムが Rust 並みの性能を出せるか」というリスクの早期検証であり、それは確認できた。
