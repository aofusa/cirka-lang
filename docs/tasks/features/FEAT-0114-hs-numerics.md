# FEAT-0114: 数値型と Integer

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0111
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

Int/Word/Double/Float/Integer/Rational と数値型クラス、show/read の GHC 互換、libm。

## 作業内容

- [x] ランタイムの多倍長整数
- [x] 数値型クラス
- [x] 浮動小数の show/read
- [x] libm

## 受け入れ条件

- [x] `product [1..100] :: Integer` 等の差分テスト
- [x] 浮動小数の表示が GHC と一致
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `Integer` は `IS !Int | IP !BigNat` の正規形。多倍長演算は新クレート `cirka-bigint` (no_std・割り当てなし、Knuth D の除算、
  `f64` との正確な変換) をランタイムと MIR インタプリタで共有する (ADR-034)。GMP は使わない。
- `Float` は `Double` を包み演算ごとに単精度へ丸める。`show` は単精度の最短表現 (`0.33333334`)。`Word` は `Int` を包む符号なし。
- 小数リテラルは最短 10 進表現をそのまま (任意の大きさの) 既約分数にする (`3.4028235e38 :: Float`・`1e300 :: Rational`)。
- `Data.Ratio`・`Data.Word` を追加。libm (`sin` など) は FEAT-0110 で既に glibc を呼んでいる。
- テスト: `tests/achc/run/h2/numeric.hs` (`product [1..100]`、多倍長の div/mod/quot/rem、`read`/`show`、Float/Double の表示、
  `decodeFloat`・`toRational`・`realToFrac`、Word)。Docker 内で GHC 9.4.7 と一致。
- `Natural` は未実装 (必要になった時点で追加)。

