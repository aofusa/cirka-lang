# FEAT-0004: CI スクリプト

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P1
- 依存: FEAT-0003
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

`scripts/ci.sh` と `memcheck.sh` / `compat.sh` / `bench.sh` の雛形。

## 作業内容

- [ ] `scripts/ci.sh` (fmt・clippy・build・test)
- [ ] `scripts/memcheck.sh` (valgrind)
- [ ] `scripts/compat.sh` / `scripts/bench.sh` の雛形

## 受け入れ条件

- [ ] コンテナ内で `scripts/ci.sh` が成功
- [ ] いずれかの段階が失敗すると非ゼロで終了
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

