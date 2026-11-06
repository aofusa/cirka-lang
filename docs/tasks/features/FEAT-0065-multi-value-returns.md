# FEAT-0065: 複数の値の返却 (組の結果の CPR)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0041, FEAT-0056
- 設計: [06-ir.md](../../design/06-ir.md), [16-performance.md](../../design/16-performance.md) §2

## 概要

一意な値を使う API は `(Unique a, Ur b)` のように組を返す (`A.read`、`A.length`、利用者の再帰関数)。
インライン化される関数では組は消える (FEAT-0064) が、再帰関数の結果の組と、その中の `Ur` と `I#` は呼び出しごとに確保される。
fannkuch-redux `own.chs` では 1 順列あたり約 9 回で、Rust との差 (約 3.4 倍) の主な原因。

worker/wrapper の CPR を、1 つの構築子だけを持つ型 (組・`Ur`・`Unique`) の結果に広げ、worker が複数の値を返すようにする。

- Core: 箱なしの組 (複数の値の返却) を表す方法 (例: 結果の型 `(# a, b #)` と、それを受ける `case`)。Lint の規則
- MIR: 複数の結果を持つ関数と呼び出し (または呼び出し側の領域へ書き込む出力引数)
- LLVM: 構造体の返却 (`{ptr, i64}` はレジスタで返る)
- 入れ子の箱 (`(u, Ur (I# x))`) は平らにする: `(# u, x #)`

## 作業内容

- [x] 設計 (06-ir.md §1.7・§2.2、ADR-049)
- [x] Core・MIR・LLVM の表現と検証器 (Core: `UTup<n>` と Lint の置き場所の規則 `cpr::check_placement`。MIR: `Function.rets`・`Instr::LetMulti`・`Terminator::RetMulti`、検証器・Perceus・インタプリタ・テキスト形式。LLVM: 構造体の返却)
- [x] worker/wrapper の CPR を組に広げる (`crates/cirka-core/src/cpr.rs`、組の結果の形の最大不動点・入れ子の平坦化)
- [x] fannkuch-redux `own.chs`・array-sort `own.chs`・n-body `own.chs` で計測

## 受け入れ条件

- [x] `tests/achc/run/h3/fannkuch_unique.hs` の `max-allocs` を、順列の数に比例しない値まで下げる (50000 → 1000。実際は 143)
- [x] テストを追加し `scripts/ci.sh` が通る

## 実装のメモ

- wrapper は内側の構築子を先に `let!` で束縛して組み立てる (`Tuple2 r v`、`v = Ur w`、`w = I# x`)。入れ子のまま (`Tuple2 r (Ur (I# x))`) だと、
  線形な変数を含む構築子の `let!` を簡約器が既知の構築子にしない。
- float-in: 原子に適用した構築子の `let!` も遅延 `let` と同じく沈める。float-in と簡約は 2 回まで繰り返す (1 回目の写しが簡約で消えた後に、出口だけで使う箱を沈める)。
- CPR の後にもう一度 worker/wrapper を行う (インライン化した wrapper が評価済みの値を渡すようになった引数を unboxing する)。既に分割した関数は分割しない。
- strictify が呼び出しの前に引数を `let!` で評価するので、`case` で分解する箱なしの組は「`let`/`let!` の後の既知関数の呼び出し」まで許す (Lint と MIR 変換)。
- 1 つの結果を返す worker (unboxed の値) で、LLVM 18 の TailCallElim が `musttail` を壊す場合がある (07-backend.md §2)。

## 計測 (2026-11-06、ホスト、3 回の最良値)

| ベンチ | 前 (FEAT-0064) | 後 | Rust |
|--------|---:|---:|---:|
| fannkuch-redux `own.chs` (n = 9) | 0.099 s (確保 3,164,209) | **0.055 s** (確保 521) | 0.029 s |
| array-sort `own.chs` | 0.43 s | 0.37 s | 0.088 s |
| n-body `main.cirka` | 5.5 s | 4.5 s | |
| n-body `own.chs`・spectral-norm・mandelbrot | | 変化なし | |
| binary-trees (深さ 18、交互に 7 回の最良値) | `.cirka` 2.05 s / `.chs` 1.71 s | 1.78 s / 1.36 s | |

- fannkuch-redux `own.chs` は Rust の 1.9 倍 (FEAT-0064 の後は 3.4 倍)。1 順列あたりの確保は 0 になった。
  残りの差は境界検査 (FEAT-0234) と、関数の入口の一意性検査 (FEAT-0062) による。
- 計測中のホストの負荷が高く、binary-trees の 21 は値がばらついた (11〜18 s) ため、深さ 18 で交互に計測した。確保回数は前後で同じ。

