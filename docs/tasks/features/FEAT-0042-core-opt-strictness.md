# FEAT-0042: Core 最適化の基本と正格性解析

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P0
- 依存: FEAT-0036
- 設計: [06-ir.md](../../design/06-ir.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

簡約器 (β・case-of-known-constructor・let 浮動・インライン) と需要解析による正格性解析、`Strict`/`StrictData`。

## 作業内容

- [ ] 簡約器
- [ ] 需要解析と正格な let・引数への変換
- [ ] `seq`/`$!`/BangPatterns/`Strict`/`StrictData`
- [ ] 各パス後の Core Lint

## 受け入れ条件

- [ ] `sum [1..10^7]` 相当のループでサンクが作られないことを確保回数で確認
- [ ] 最適化の前後で run テストの結果が変わらない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

