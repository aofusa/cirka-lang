# FEAT-0100: Haskell 字句解析とレイアウト規則

- 状態: todo
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0005
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の字句解析とレイアウト規則 (§10.3、parse-error(t) を含む)。

## 作業内容

- [ ] トークン
- [ ] レイアウトアルゴリズム
- [ ] `{-# LANGUAGE #-}` pragma の収集

## 受け入れ条件

- [ ] Report の例と入れ子レイアウトのスナップショット
- [ ] parse-error(t) が必要なケース
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

