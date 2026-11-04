# FEAT-0037: achc ドライバと CLI

- 状態: todo
- マイルストーン: M3 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0036, FEAT-0003
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

`achc build/run/check`、`--dump`、`-O`、最小の `achc.toml` 読み込み。run テストをハーネスにつなぐ。

## 作業内容

- [ ] パイプライン実行と中間生成物 (`_build/`)
- [ ] `--dump=` 各段階
- [ ] `achc.toml` の `[package]` と `[compiler]`
- [ ] run/fail テストの有効化

## 受け入れ条件

- [ ] `tests/run/m3/` (fact, fib, map, 遅延リスト、文字列出力) が `achc run` で GHC と同じ出力・リーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

