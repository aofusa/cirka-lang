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

## CirkaLang v0.0.1 (FEAT-0207)

計測: `bench/run.sh binary-trees n-body` (Docker コンテナ内、5 回の平均)。CirkaLang は `cirka build -O2`。
出力は Rust 版と一致する (`tests/cirkalang.rs` の `cirkalang_bench_programs` が小さい引数で確認)。

### binary-trees (深さ 21, 単一スレッド)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| 手書き MIR (`cirka-mir build -O2`) | 15.34 s ± 0.11 | 0.68 |
| 手書き MIR + Perceus (`--perceus`) | 15.29 s ± 0.07 | 0.68 |
| **CirkaLang v0.0.1** (`bench/binary-trees/main.cirka`) | 36.29 s ± 0.02 | 1.60 |
| Rust (`Option<Box<Node>>`) | 22.65 s ± 0.15 | 1.00 |

### n-body (5,000,000 ステップ)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| **CirkaLang v0.0.1** (`bench/n-body/main.cirka`) | 20.52 s ± 0.06 | 52.9 |
| Rust (f64 の構造体の配列、SIMD なし) | 0.388 s ± 0.006 | 1.00 |

### 分析

- 主因は **数値のボックス化**。v0.0.1 の `Int` / `Float` は Core で `I# Int#` / `F# Double#` のデータ型であり、
  共通 Core の最適化にまだ worker/wrapper・unboxing がないので、算術の結果ごとに 1 回割り当てる。
  - binary-trees: 木の節 1 つに対して `check` の戻り値の `I#` が 2 つ割り当てられ、`make` の引数の `d - 1` も箱に入る。
    木そのものの構築と解放は手書き MIR と同じ (Perceus・静的な `Leaf`) で、差はほぼ数値の割り当て。
  - n-body: 1 ステップで 10 組 × 約 20 回の浮動小数点演算がすべて `F#` を割り当てる。
    さらに配列と破壊的更新がないので、5 体のリストと `Body` (7 フィールド) を毎ステップ作り直す
    (Perceus の再利用で大半はその場の更新になるが、フィールドの `F#` は新しい箱)。
- 対策 (次版以降の設計に反映):
  - worker/wrapper と unboxing: 正格な `Int`/`Float` 引数・戻り値を `Int#`/`Double#` で渡す (FEAT-0041、v0.0.3 で G3 を目指す)。
    CirkaLang は正格なので、aCHC と違って正格性解析なしで常に unboxing できる。
  - 配列と破壊的更新 (一意な配列、FEAT-0211、v0.0.2) で n-body をRust と同じデータ構造で書けるようにする。
- 結論: GC なしのメモリ管理 (Perceus + 本ランタイム) 自体は手書き MIR で Rust より速いことを M1 で確認済みであり、
  v0.0.1 の差はフロントエンドの出力するボックス化された数値によるもの。v0.0.3 で再計測する。

## Core 最適化 (FEAT-0041)

計測: `bench/run.sh binary-trees n-body` (Docker コンテナ内、`CIRKA_BENCH_RUNS=3`)。CirkaLang は `cirka build -O2`。
変更点は worker/wrapper (正格な箱型の引数・戻り値の unboxing)・正格な箱型フィールドの展開・case-of-case ([06-ir.md](06-ir.md) §1.4)。

### binary-trees (深さ 21)

| 実装 | 平均時間 | 比 (Rust=1) | v0.0.1 |
|------|---------:|------:|------:|
| 手書き MIR (`cirka-mir build -O2`) | 15.32 s ± 0.02 | 0.68 | |
| 手書き MIR + Perceus | 15.36 s ± 0.11 | 0.69 | |
| **CirkaLang** (`main.cirka`) | 12.34 s ± 0.35 | **0.55** | 36.29 s (1.60) |
| Rust | 22.40 s ± 0.05 | 1.00 | |

### n-body (5,000,000 ステップ)

| 実装 | 平均時間 | 比 (Rust=1) | v0.0.1 |
|------|---------:|------:|------:|
| **CirkaLang** (`main.cirka`) | 13.67 s ± 0.01 | **34.7** | 20.52 s (52.9) |
| Rust | 0.394 s ± 0.006 | 1.00 | |

### 確保回数 (`CIRKA_STATS=1`)

| プログラム | 変更前 | 変更後 |
|------|------:|------:|
| binary-trees (深さ 14) | 9,688,585 | 3,222,349 |
| n-body (100,000 ステップ) | 19,300,000 前後 | 13,200,080 |
| aCHC `tests/achc/run/h1/fib.hs` | 88,739 | 728 |
| aCHC `lazy.hs` | 4,131 | 2,199 |
| aCHC `strings.hs` | 1,996 | 1,355 |
| aCHC `map.hs` | 1,204 | 931 |
| aCHC `fact.hs` | 641 | 441 |

### 分析

- binary-trees は数値の箱がほぼ消え、手書き MIR より速くなった (手書き MIR は `check` の結果を毎回箱に入れる書き方のまま)。
- n-body の残りの割り当ては、1 組の相互作用ごとの `Pair` (戻り値の組) とリストのセル。組を返す関数の CPR には複数値の戻り (unboxed タプル) が要るので、
  配列と破壊的更新 (FEAT-0211, v0.0.2) で Rust と同じデータ構造に書き直すのと合わせて再計測する。
