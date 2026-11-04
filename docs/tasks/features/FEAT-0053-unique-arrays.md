# FEAT-0053: 一意配列 (Cirka.Array) と in-place 更新

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P0
- 依存: FEAT-0051, FEAT-0022
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

`Unique (Array e)` / 非ボックス配列の in-place 操作と、freeze/thaw。

## 作業内容

- [ ] Array / UArray (非ボックス) のランタイム表現
- [ ] new/read/write/length/freeze/thaw
- [ ] LType 値の再利用を実行時チェックなしで行う

## 受け入れ条件

- [ ] 配列を使うベンチ (in-place ソート等) が Rust の 1.5 倍以内
- [ ] 書き込み時に確保が発生しないことを確保回数で確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

