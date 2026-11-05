# FEAT-0128: 線形性の推論 (LType 一般化)

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P2
- 依存: FEAT-0121
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型変数を LType に一般化できる場合の推論。

## 作業内容

- [x] 使用量に基づく LType 一般化 (試行検査、ADR-045)
- [x] U-Arg の多重度を一般化の後に決める

## 受け入れ条件

- [x] 注釈なしで `swap` 等が LType に使える
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- シグネチャのない関数の束縛群の一般化時に、型変数ごとに `LType` を仮定して `linear.rs` の検査器で本体を試行検査し、
  誤りが増えず `Type` の位置 (E0310 の対象の参照・無制限の矢印への引数) に渡していなければ `LType` に一般化する。
  合格した変数をまとめて再試行し、失敗すればすべて `Type` (03 §2.4)。
- 例: `swap (x, y) = (y, x)` → `(a, b) %1 -> (b, a)` (`a`, `b` は LType)。`map swap xs` も通る。
- 推論しないもの: 多重度多相 (`apply f x = f x` の `f` は ω)、ワイルドカードで捨てる `first (x, _) = x` (E0302 になるため `Type`)。
- テスト: `tests/achc/typeck/{ok,fail}/linearity_infer.hs`、`tests/achc/run/h3/linear_infer.hs`。

