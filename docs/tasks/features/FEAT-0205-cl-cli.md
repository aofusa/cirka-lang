# FEAT-0205: cirka CLI と cirka.toml

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0204, FEAT-0031, FEAT-0032
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

`cirka build/run/check`、`cirka.toml`、`cirka --version` (v0.0.1)。`tests/cirkalang/{run,fail}`。

## 作業内容

- [ ] CLI
- [ ] `cirka.toml` の `[package]`
- [ ] テスト種別の有効化

## 受け入れ条件

- [ ] `tests/cirkalang/run/v001/` (fib, map, ADT, 再帰) がリーク 0 で動く
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

