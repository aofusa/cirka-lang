# FEAT-0123: 一意配列 (Cirka.Array)

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0121, FEAT-0022
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

`Unique (Array e)` の in-place 操作と freeze/thaw。

## 作業内容

- [ ] Array / UArray のランタイム表現
- [ ] new/read/write/length/freeze/thaw

## 受け入れ条件

- [ ] 書き込み時に確保が発生しない
- [ ] in-place ソートのベンチを記録
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

