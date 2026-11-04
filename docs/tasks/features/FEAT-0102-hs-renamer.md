# FEAT-0102: Haskell 名前解決とモジュール

- 状態: done
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0101
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

モジュールグラフ、import/export、スコープ、Name の割り当て、fixity の再結合。

## 作業内容

- [x] モジュール探索
- [x] qualified/hiding/as
- [x] fixity による再結合
- [x] 未定義・曖昧な名前の診断

## 受け入れ条件

- [x] 複数モジュールのテスト
- [x] エラーテスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `crates/achc-rename`: モジュールを読み込んで (`A.B` → `A/B.hs` / `.chs`) import グラフをトポロジカル順に並べ、1 モジュールずつ名前解決する。
  import の循環はエラー。`NoImplicitPrelude` がなく `Prelude` を明示的に import していなければ `Prelude` を暗黙に import する。
- 出力は構文木のまま名前を解決したもの: 大域名は定義モジュールで修飾した `QName`、局所名は修飾なしの一意な名前 `x#N`
  (モジュールごとに採番、表示は `display_name` で元に戻す)。組み込みの構文 (`[]` `:` `()` タプル `->`) は `Cirka.Prim` に属する。
- import/export: `qualified` / `as` / `hiding`、`T(..)` / `T(a, B)`、`module M` の再 export。曖昧な名前 (E0202)・未定義 (E0201)・
  重複 (E0203)・項目なし (E0205)・シグネチャだけの名前 (E0606)。サポート外の拡張は E0001。
- 結合性: Report §10.6 の解決アルゴリズム (前置 `-` は `Prelude.negate`、同じ優先度で結合方向が違えば E0204)。局所の結合性宣言にも対応。
- テスト: `achc-rename` の単体テストと `tests/achc/modules/{qualified,scope_errors}` (`achc_modules`)。
