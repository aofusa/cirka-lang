# FEAT-0100: Haskell 字句解析とレイアウト規則

- 状態: done
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0005
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の字句解析とレイアウト規則 (§10.3、parse-error(t) を含む)。

## 作業内容

- [x] トークン
- [x] レイアウトアルゴリズム
- [x] `{-# LANGUAGE #-}` pragma の収集

## 受け入れ条件

- [x] Report の例と入れ子レイアウトのスナップショット
- [x] parse-error(t) が必要なケース
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `crates/achc-syntax/src/lexer.rs`: Haskell 2010 の字句 (修飾名・演算子・`--` と入れ子の `{- -}`・数値/文字/文字列リテラルとエスケープ・文字列のギャップ)。
  列はタブを 8 桁境界に揃えて数える。`{-# LANGUAGE ... #-}` の拡張名を集める (サポート外の拡張の E0001 は名前解決以降で出す)。
- レイアウト規則は **パーサが駆動する** (`parser.rs` の `cur` / `open_block` / `block`)。仮想の `{` `;` `}` をその場で計算し、
  ブロックの要素を続けられないトークンで暗黙のブロックを閉じる (parse-error(t))。`let .. in` の一行書き・括弧の中の `case`・
  リスト内包の `let`・DoAndIfThenElse を扱う。
- テスト: `achc-syntax` の単体テスト (Report §10.3 の例のスナップショット、parse-error(t)、字句) と
  `tests/achc/parse/layout.hs`。診断コード E0101 / E0102 を 03-type-system.md §11 に追記。
