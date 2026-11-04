# FEAT-0020: Perceus: dup/drop の自動挿入

- 状態: todo
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0011
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

dup/drop を含まない MIR から、精密な dup/drop を挿入するパスを作る (Reinking et al. 2021)。

## 作業内容

- [ ] 生存変数解析
- [ ] 最後の使用での所有権移動・分岐先頭での drop・フィールド読み出し時の dup
- [ ] 自動挿入した MIR を検証器に通す

## 受け入れ条件

- [ ] `tests/mir/perceus/` の入力から期待どおりの dup/drop が挿入される (スナップショット)
- [ ] 挿入後の全テストがリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

