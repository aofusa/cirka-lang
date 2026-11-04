# FEAT-0101: Haskell 構文解析

- 状態: done
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0100
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

Haskell 2010 の構文を AST に変換する (fixity 解決は名前解決で)。

## 作業内容

- [x] モジュール・import/export・宣言
- [x] 式・パターン・型
- [x] エラー回復

## 受け入れ条件

- [x] Report の文法を網羅するスナップショット
- [x] 不正入力でパニックしない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `crates/achc-syntax/src/{ast,parser,print}.rs`: モジュール・export/import・data/newtype (レコード・正格フィールド・中置コンストラクタ・deriving)・
  type・class・instance・default・foreign・型シグネチャ・結合性宣言・関数の節 (中置定義・ガード・where)・パターン束縛、
  式 (ラムダ・let・if・MultiWayIf・case・`\case`・do・タプルとタプルセクション・リスト・範囲・内包・左右のセクション・型注釈・レコード構築/更新)、
  パターン (as・遅延・バン・負のリテラル・レコード・中置コンストラクタ)、型 (文脈・forall)。
- 演算子の並びは `OpSeq` のまま残し、結合性の解決は名前解決 (FEAT-0102) で行う。
- エラー回復: ブロックの要素単位で読み飛ばす。不正な入力でパニックしないことを単体テストで確認。
- テスト: `tests/achc.rs` の `achc_parse_snapshots` (`tests/achc/parse/*.hs` → `.ast`) と `achc_syntax_errors` (`tests/achc/fail/syntax`)。
