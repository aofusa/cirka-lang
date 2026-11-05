# FEAT-0118: GHC 拡張 (初期分)

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0110
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

差分テスト・cirka-base に必要な拡張だけを追加する (02 §1.2 の初期分)。GADTs・型族などの高度な型拡張は H5 (FEAT-0140〜)。

## 作業内容

- [x] BangPatterns, ScopedTypeVariables, LambdaCase, TupleSections, MultiWayIf
- [x] RankNTypes (注釈必須), MPTC, FlexibleInstances/Contexts, GND
- [x] KindSignatures (02 §1.2 の一覧にあるため。宣言の頭部・`forall` の束縛子・型中の `(t :: k)`、`Data.Kind (Type)`)

## 受け入れ条件

- [x] 各拡張の run / fail テスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実行テスト: `tests/achc/run/h2/ext_{syntax,classes,newtype_deriving,rank_scoped,kinds}.hs` (Docker 内で GHC 9.4.7 の出力と一致)。
  診断: `tests/achc/typeck/fail/extensions.hs` (多引数クラスの引数の数、GND の表現型にインスタンスがない・多引数クラス、
  多引数の制約のインスタンスなし、rank-N の引数の型の不一致)。
- 多引数クラスは 1 引数の制約 `C (#MP2 a b)` として解く (ADR-036、03 §7.2)。関数従属・多引数クラスのスーパークラスは未対応 (H5)。
- GND は表現型の辞書のキャスト (`tast::Expr::Coerce`)。`Functor` のような高階の種のクラスは newtype の末尾の引数を η 簡約する。
- ScopedTypeVariables は明示的な `forall` だけがスコープを作る (GHC と同じ)。RankNTypes は入れ子の `forall` 型の引数を
  使うたびに具体化し、`forall` 型に対する検査は局所の多相 `let` にする。入れ子の `forall` の下の制約は未対応。
