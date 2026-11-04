# FEAT-0001: Cargo ワークスペースとクレート雛形

- 状態: todo
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: なし
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

01-architecture.md §3 のクレート構成で Cargo ワークスペースを作り、空のクレートと `achc --version` を用意する。

## 作業内容

- [ ] ルート `Cargo.toml` (workspace, edition 2024, 共通 lints)
- [ ] `crates/` 以下に各クレートの雛形 (lib.rs / main.rs)
- [ ] `cirka-runtime` を `staticlib` + `#![no_std]` で作成 (panic handler 含む)
- [ ] `achc --version` が `aCHC <version>` を表示

## 受け入れ条件

- [ ] `cargo build --workspace` と `cargo test --workspace` が成功
- [ ] `cargo clippy --all-targets -- -D warnings` が通る
- [ ] クレート間の依存方向が 01-architecture.md の表どおり
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

