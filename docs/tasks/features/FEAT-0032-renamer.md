# FEAT-0032: 名前解決とモジュール

- 状態: todo
- マイルストーン: M3 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0031
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

モジュールグラフの構築、import/export、スコープ、一意な Name の割り当て、fixity の再結合。

## 作業内容

- [ ] モジュール探索 (source-dirs)
- [ ] qualified/hiding/as
- [ ] fixity 宣言による演算子式の再結合
- [ ] 未定義・曖昧な名前の診断

## 受け入れ条件

- [ ] 複数モジュールのテスト
- [ ] エラーテスト (未定義名・曖昧な import・重複定義)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

