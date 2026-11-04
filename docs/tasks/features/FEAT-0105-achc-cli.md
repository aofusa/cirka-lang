# FEAT-0105: achc CLI と achc.toml

- 状態: todo
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0104, FEAT-0031, FEAT-0032
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

`achc build/run/check`、`achc.toml`、aCHC 用の run/fail テストの有効化。

## 作業内容

- [ ] フロントエンドと共通ドライバの接続
- [ ] `achc.toml` の `[package]`/`[compiler]`
- [ ] `tests/achc/run`・`tests/achc/fail`

## 受け入れ条件

- [ ] `tests/achc/run/h1/` (fact, fib, map, 遅延リスト, 文字列出力) が GHC と同じ出力・リーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

