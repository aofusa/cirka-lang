# FEAT-0101: Haskell 構文解析

- 状態: todo
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0100
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の構文を AST に変換する (fixity 解決は名前解決で)。

## 作業内容

- [ ] モジュール・import/export・宣言
- [ ] 式・パターン・型
- [ ] エラー回復

## 受け入れ条件

- [ ] Report の文法を網羅するスナップショット
- [ ] 不正入力でパニックしない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

