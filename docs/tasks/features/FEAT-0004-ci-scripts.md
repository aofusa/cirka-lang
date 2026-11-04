# FEAT-0004: CI スクリプト

- 状態: todo
- マイルストーン: M0 基盤整備
- 優先度: P1
- 依存: FEAT-0003
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

`scripts/ci.sh` (fmt・clippy・build・test) と、`memcheck.sh` / `compat.sh` / `bench.sh` の雛形を作る。

## 作業内容

- [ ] `scripts/ci.sh`
- [ ] `scripts/memcheck.sh` (valgrind で run テストを実行)
- [ ] `scripts/compat.sh` / `scripts/bench.sh` の雛形

## 受け入れ条件

- [ ] コンテナ内で `scripts/ci.sh` が成功する
- [ ] いずれかの段階が失敗すると非ゼロで終了する
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

