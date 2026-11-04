# FEAT-0033: Core 簡約器とインライン

- 状態: done
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P1
- 依存: FEAT-0030
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

β 簡約・case-of-known-constructor・let 浮動・インライン・DCE。各パス後に Core Lint。

## 作業内容

- [x] 簡約器
- [x] インライン判定 (多重度 1 の情報を利用)
- [x] 各パス後の Lint

## 受け入れ条件

- [x] 最適化の前後で実行結果が変わらない
- [x] 確保回数の削減を計測
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ


- 実装: `cirka-core::opt` (uniquify → simplify ×最大6 → dce)。内容は [06-ir.md §1.3](../../design/06-ir.md)。各パス後に Lint (`lint::lint_bind` を追加)。
- テスト: `cirka-core` の単体テスト (β・定数畳み込み・インライン・case-of-known・正格性/効果の保持・単一使用 thunk・線形性)、`tests/core.rs::core_opt_reduces_allocations` (tests/core/run 全体で最適化前後の出力・終了コード・リーク 0・確保回数が増えないこと)、`tests/driver.rs` (-O0 / -O2 のネイティブ実行一致)。
- 確保回数 (MIR インタプリタ):

  | プログラム | 最適化前 | 最適化後 |
  |---|---|---|
  | opt_inline (ラッパ・タプル・高階関数のループ 1000 回) | 10001 | 4001 |
  | closures | 10 | 3 |
  | haskell_style (型クラス辞書) | 101 | 99 |
  | cirkalang_style / data_misc / panic | 9 / 12 / 0 | 9 / 12 / 0 |
