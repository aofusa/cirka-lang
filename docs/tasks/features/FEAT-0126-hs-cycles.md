# FEAT-0126: 循環の検出と局所回収器

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P2
- 依存: FEAT-0112
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

再帰的な値束縛の静的検出 (W0401)、`--cycles=warn|error|collect`、試行削除。

## 作業内容

- [ ] W0401
- [ ] 循環候補の記録
- [ ] 試行削除

## 受け入れ条件

- [ ] `let xs = 1 : xs` で警告、`--cycles=collect` でリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

