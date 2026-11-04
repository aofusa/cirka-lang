# FEAT-0011: MIR 所有権検証器

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0010
- 設計: [06-ir.md](../../design/06-ir.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

06-ir.md §2.3 の検証器。dup/drop の誤りを静的に検出する。

## 作業内容

- [ ] 型・スコープ検査
- [ ] 所有権データフロー解析
- [ ] owned の消費回数・borrowed の消費禁止・再利用トークンの検査
- [ ] 分かりやすい診断

## 受け入れ条件

- [ ] `tests/mir/fail/` の誤り (二重 drop・drop 漏れ・解放後使用・borrowed の消費) をすべて検出
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

