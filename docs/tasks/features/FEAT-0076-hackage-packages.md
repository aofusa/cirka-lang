# FEAT-0076: Hackage パッケージの移植

- 状態: todo
- マイルストーン: M7 互換性と性能
- 優先度: P2
- 依存: FEAT-0075
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

最小 Cabal 解釈器と、transformers / mtl / containers などの移植。

## 作業内容

- [ ] `.cabal` の最小解釈
- [ ] 依存パッケージのビルド
- [ ] パッチ管理 (`lib/patches/`)

## 受け入れ条件

- [ ] transformers・mtl・containers のテストが通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

