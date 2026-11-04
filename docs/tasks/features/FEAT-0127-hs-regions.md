# FEAT-0127: リージョン (aCHC)

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P2
- 依存: FEAT-0124, FEAT-0049
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

`withRegion`/`RIO r`/`allocIn`/`readIn`/`newRefIn`。

## 作業内容

- [ ] API と構文糖

## 受け入れ条件

- [ ] リージョン内の循環グラフがリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

