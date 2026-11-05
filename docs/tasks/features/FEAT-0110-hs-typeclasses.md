# FEAT-0110: 型クラスと deriving

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0105
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型クラス・インスタンス・スーパークラス・デフォルトメソッド・defaulting、辞書渡し、deriving。

## 作業内容

- [x] 制約の生成と解決 (頭部正規形、スーパークラス、外側への持ち越し、単相性制限)
- [x] 辞書渡し (クラス = Core のデータ型、インスタンス = 辞書関数、デフォルトメソッド)
- [x] Haskell 2010 の defaulting (`Integer` (= `Int`) → `Double`)
- [x] deriving (Eq, Ord, Show, Read, Enum, Bounded)
- [x] Monad による do 記法 (失敗しうるパターンは MonadFail の `fail`)
- [x] クラス版 Prelude (`Cirka/Base.hs` に実体、`Prelude.hs` は GHC と同じ名前を再エクスポート)

## 受け入れ条件

- [x] 型クラスの run / 型エラーテスト (`tests/achc/run/h2/classes.hs`、`tests/achc/typeck/fail/classes.hs`、`achc-typeck` の単体テスト)
- [x] deriving Show/Read の出力が GHC と一致 (`tests/achc/run/h2/deriving.hs`。期待出力は Docker 内の GHC の出力と同一)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: 03-type-system.md §7.1、09-libraries.md §1.2、ADR-032。
- 途中で見つけたバグ: BUG-0003 (クロージャの入口スタブの衝突、修正済み)、BUG-0004 (網羅性検査の誤警告、未修正)。
- `IO` を Core の newtype にしたため、`IO` プリミティブは `foreign import prim` の束縛でキャストする。

