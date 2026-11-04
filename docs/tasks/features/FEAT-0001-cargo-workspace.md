# FEAT-0001: Cargo ワークスペースとクレート雛形

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: なし
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

01-architecture.md §3 のクレート構成 (共通 `cirka-*`、aCHC `achc-*`、CirkaLang `cirkalang-*`) で Cargo ワークスペースを作る。

## 作業内容

- [ ] ルート `Cargo.toml` (workspace, edition 2024, 共通 lints)
- [ ] `crates/` 以下に各クレートの雛形
- [ ] `cirka-runtime` を `staticlib` + `#![no_std]` で作成
- [ ] `achc --version` と `cirka --version` (`CirkaLang v0.0.1-dev`) を表示

## 受け入れ条件

- [ ] `cargo build --workspace` / `cargo test --workspace` / `cargo clippy --all-targets -- -D warnings` が成功
- [ ] クレート間の依存方向が 01-architecture.md どおり (共通クレートはフロントエンドに依存しない)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

