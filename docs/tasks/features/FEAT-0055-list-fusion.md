# FEAT-0055: リスト融合 (foldr/build)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0112
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

FEAT-0112 の後も、`sum [1 .. n]` のようにリストを作ってすぐ消費するプログラムでは、要素ごとにコンスセル・箱・遅延した尾のサンクが確保される
(`sum [1 .. 10^7]` で確保 3×10^7 回、評価されるサンク 10^7 個)。GHC は foldr/build 融合でこれを消す。
生産者 (`enumFromTo`・`map`・`filter` …) と消費者 (`foldr`・`foldl'`・`sum` …) を Core の書き換えで融合し、中間リストを作らないようにする。

## 作業内容

- [ ] 融合の方式を決める (GHC の foldr/build 規則か、stream fusion か) と設計書 (06-ir.md) への追記
- [ ] `Cirka.Base` の生産者・消費者を融合できる形で定義
- [ ] Core の書き換え (規則) の機構

## 受け入れ条件

- [ ] `sum [1 .. 10^7 :: Int]` で確保回数が定数 (`max-allocs` のテスト)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

FEAT-0112 で発見 (tests/achc/run/h2/strictness.hs の確保回数の内訳)。
