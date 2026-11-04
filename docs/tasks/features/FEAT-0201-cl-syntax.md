# FEAT-0201: CirkaLang 字句・構文解析

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0005, FEAT-0200
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

レイアウトベースの構文 (Haskell/Idris 風、型注釈は `:`) の字句解析と構文解析。

## 作業内容

- [x] 字句解析とレイアウト
- [x] 宣言・式・パターン・型 (依存関数型 `(x : A) -> B`、多重度 `(1 x : A)`)
- [x] エラー回復

## 受け入れ条件

- [x] スナップショットテスト
- [x] 不正入力でパニックしない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ


- 実装: `cirkalang-syntax` の `lexer` (修飾名・リテラル・入れ子コメント)、`layout` (spec §2.1。暗黙ブロックの開閉、`in`、括弧と `,`、`if`/`case` の `then`/`else`/`of`)、`parser` (再帰下降。演算子列はそのまま残し、結合は名前解決で fixity を使って行う)、`print` (S 式ダンプ)。
- エラー回復: 宣言 (トップレベルの行頭) とブロックの要素の単位で読み飛ばして続ける。予約語 (`total` `trait` など) は E1004。
- テスト: 単体テスト (字句・レイアウト・宣言・式・回復、切り詰めと記号の挿入/削除 3000 通りでパニックしないこと)、`tests/cirkalang/parse/*.ast` (仕様書 §8 の例を含む) と `tests/cirkalang/fail/syntax/*.stderr` のスナップショット (`tests/cirkalang.rs`)。
