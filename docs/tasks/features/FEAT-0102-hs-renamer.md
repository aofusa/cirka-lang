# FEAT-0102: Haskell 名前解決とモジュール

- 状態: todo
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0101
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

モジュールグラフ、import/export、スコープ、Name の割り当て、fixity の再結合。

## 作業内容

- [ ] モジュール探索
- [ ] qualified/hiding/as
- [ ] fixity による再結合
- [ ] 未定義・曖昧な名前の診断

## 受け入れ条件

- [ ] 複数モジュールのテスト
- [ ] エラーテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

