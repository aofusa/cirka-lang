# FEAT-0031: 構文解析 (Haskell 2010)

- 状態: todo
- マイルストーン: M3 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0030
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の構文を AST に変換する。演算子は列のまま保持し、fixity 解決は名前解決で行う。

## 作業内容

- [ ] モジュール・import/export・宣言 (data/newtype/type/class/instance/型シグネチャ/fixity)
- [ ] 式・パターン・型
- [ ] エラー回復と分かりやすい構文エラー

## 受け入れ条件

- [ ] Report の文法を網羅するスナップショットテスト
- [ ] 不正な入力でパニックしない (簡易ファズ)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

