# FEAT-0001: Cargo ワークスペースとクレート雛形

- 状態: done
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: なし
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

01-architecture.md §3 のクレート構成 (共通 `cirka-*`、aCHC `achc-*`、CirkaLang `cirkalang-*`) で Cargo ワークスペースを作る。

## 作業内容

- [x] ルート `Cargo.toml` (workspace, edition 2024, 共通 lints)
- [x] `crates/` 以下に各クレートの雛形
- [x] `cirka-runtime` を `staticlib` + `#![no_std]` で作成
- [x] `achc --version` と `cirka --version` (`CirkaLang v0.0.1-dev`) を表示

## 受け入れ条件

- [x] `cargo build --workspace` / `cargo test --workspace` / `cargo clippy --all-targets -- -D warnings` が成功
- [x] クレート間の依存方向が 01-architecture.md どおり (共通クレートはフロントエンドに依存しない)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 統合テストは `tests/` をワークスペースのメンバ (`cirka-integration`) とし、`[[test]]` で対象を列挙する。
- `cirka-runtime` は `no_std` の staticlib のため、ワークスペースの dev/release プロファイルを `panic = "abort"` にした (テストハーネスは cargo が unwind でビルドする)。`doctest = false`。
- テスト用の共通部品は `cirka-testkit` (外部クレートに依存しない) に置く。
