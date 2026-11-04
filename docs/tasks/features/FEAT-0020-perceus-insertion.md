# FEAT-0020: Perceus: dup/drop の自動挿入

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0011
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

dup/drop を含まない MIR から精密な dup/drop を挿入する。

## 作業内容

- [ ] 生存変数解析
- [ ] 最後の使用での移動・分岐先頭での drop・フィールド読み出し時の dup
- [ ] 挿入後に検証器を実行

## 受け入れ条件

- [ ] `tests/mir/perceus/` のスナップショット
- [ ] 挿入後の全テストがリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

