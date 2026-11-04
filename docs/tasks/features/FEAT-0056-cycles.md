# FEAT-0056: 循環の検出 (警告) と局所回収器

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P2
- 依存: FEAT-0042
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

再帰的な値束縛の静的検出 (W0401)、`--cycles=warn|error|collect`、試行削除による局所回収器。

## 作業内容

- [ ] W0401 の検出と診断
- [ ] 循環候補の記録
- [ ] 試行削除 (Bacon–Rajan)

## 受け入れ条件

- [ ] `let xs = 1 : xs` で警告、`--cycles=collect` でリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

