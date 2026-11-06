# FEAT-0065: 複数の値の返却 (組の結果の CPR)

- 状態: todo
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

- [ ] 設計 (06-ir.md と ADR)
- [ ] Core・MIR・LLVM の表現と検証器
- [ ] worker/wrapper の CPR を組に広げる
- [ ] fannkuch-redux `own.chs`・array-sort `own.chs`・n-body `own.chs` で計測

## 受け入れ条件

- [ ] `tests/achc/run/h3/fannkuch_unique.hs` の `max-allocs` を、順列の数に比例しない値まで下げる
- [ ] テストを追加し `scripts/ci.sh` が通る
