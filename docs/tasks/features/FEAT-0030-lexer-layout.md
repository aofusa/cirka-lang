# FEAT-0030: 字句解析とレイアウト規則

- 状態: todo
- マイルストーン: M3 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0005
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の字句解析とレイアウト規則 (§10.3、`parse-error(t)` 規則を含む) を実装する。

## 作業内容

- [ ] トークン (識別子・演算子・リテラル・コメント・pragma)
- [ ] レイアウトアルゴリズム
- [ ] `{-# LANGUAGE #-}` pragma の収集

## 受け入れ条件

- [ ] Haskell 2010 Report の例と、`where`/`let`/`do`/`case` の入れ子のスナップショットテスト
- [ ] `let x = 1 in x` 等 parse-error(t) が必要なケース
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

