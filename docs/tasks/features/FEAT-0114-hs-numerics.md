# FEAT-0114: 数値型と Integer

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0111
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

Int/Word/Double/Float/Integer/Rational と数値型クラス、show/read の GHC 互換、libm。

## 作業内容

- [ ] ランタイムの多倍長整数
- [ ] 数値型クラス
- [ ] 浮動小数の show/read
- [ ] libm

## 受け入れ条件

- [ ] `product [1..100] :: Integer` 等の差分テスト
- [ ] 浮動小数の表示が GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

