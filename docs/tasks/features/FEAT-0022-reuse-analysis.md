# FEAT-0022: drop 特殊化と再利用解析 (FBIP)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0020
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

drop 特殊化、`drop_reuse` による同サイズセルの再利用、dup/drop 融合。

## 作業内容

- [ ] drop 特殊化
- [ ] 再利用解析
- [ ] dup/drop 融合

## 受け入れ条件

- [ ] RC==1 のリストの `map` が新規確保なしで動く
- [ ] M1 のベンチが手書き MIR と同等
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

