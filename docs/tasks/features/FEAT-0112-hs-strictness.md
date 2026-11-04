# FEAT-0112: 正格性解析と Strict/StrictData

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0033
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

需要解析による正格性解析 (aCHC の遅延評価のコスト削減)、`seq`/BangPatterns/`Strict`/`StrictData`。

## 作業内容

- [ ] 需要解析
- [ ] 正格な let・引数への変換
- [ ] `Strict`/`StrictData`

## 受け入れ条件

- [ ] `sum [1..10^7]` 相当でサンクが作られないことを確保回数で確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

