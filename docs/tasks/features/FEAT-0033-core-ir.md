# FEAT-0033: Core IR の定義と Core Lint

- 状態: todo
- マイルストーン: M3 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0005
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

06-ir.md §1 の Core とプリティプリンタ、Core Lint (型・スコープ。多重度は FEAT-0050 で追加)。

## 作業内容

- [ ] Core のデータ構造とプリティプリンタ
- [ ] Core Lint
- [ ] 簡易な Core 構築 DSL (テスト用)

## 受け入れ条件

- [ ] 型の誤った Core を Lint が検出するテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

