# FEAT-0004: CI スクリプト

- 状態: done
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P1
- 依存: FEAT-0003
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

`scripts/ci.sh` と `memcheck.sh` / `compat.sh` / `bench.sh` の雛形。

## 作業内容

- [x] `scripts/ci.sh` (fmt・clippy・build・test)
- [x] `scripts/memcheck.sh` (valgrind)
- [x] `scripts/compat.sh` / `scripts/bench.sh` の雛形

## 受け入れ条件

- [x] コンテナ内で `scripts/ci.sh` が成功
- [x] いずれかの段階が失敗すると非ゼロで終了
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `scripts/build-runtime.sh` で release / debug の 2 種類の `libcirka_rt.a` を `target/rt`・`target/rt-debug` に作る (cargo test の実行中に cargo を入れ子で呼ばないため、CI で先に作っておく)。
- `bench.sh` は `bench/run.sh` (FEAT-0016) に委譲する。
