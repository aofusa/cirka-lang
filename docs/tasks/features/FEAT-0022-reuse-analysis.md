# FEAT-0022: drop 特殊化と再利用解析 (FBIP)

- 状態: todo
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0020
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

drop 特殊化、`drop_reuse` による同サイズセルの再利用、冗長な dup/drop の融合を実装する。

## 作業内容

- [ ] drop 特殊化 (dup したフィールドと親の drop を打ち消す)
- [ ] 再利用解析 (`drop_reuse` / `alloc ... reuse`)
- [ ] dup/drop 融合

## 受け入れ条件

- [ ] `map` が RC==1 のリストに対して新規確保なしで動くことを確保回数で確認
- [ ] M1 のベンチが手書き MIR と同等の性能
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

