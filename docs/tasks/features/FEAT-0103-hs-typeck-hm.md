# FEAT-0103: Haskell 型推論 (HM・ADT)

- 状態: done
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0102, FEAT-0030
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型クラスを除く HM 型推論。種推論・型シグネチャ・SCC・ランク1多相。結果を Core に elaborate する。

## 作業内容

- [x] 種推論
- [x] 依存解析と let 多相
- [x] 型シグネチャの検査
- [x] 型エラー診断

## 受け入れ条件

- [x] 型エラーテスト一式
- [x] 推論結果のスナップショット
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装: `crates/achc-typeck` (`infer.rs` が推論と elaboration、`tast.rs` が型付き AST、`types.rs` が型と表示)。
  出力は Core ではなく **型付き AST (TAST)** とし、Core への変換とパターンマッチのコンパイルは FEAT-0104 (achc-desugar) で行う。
  TAST では構文糖 (if / do / 内包表記 / 範囲 / セクション / タプル / リスト / レコード) を `Match`・`Lam`・`App` と
  `Prelude.>>=` / `Prelude.concatMap` / `Prelude.enumFrom*` の呼び出しに展開済みで、残るのはパターン照合だけ。
- 推論: レベル付き HM。トップレベル (全モジュール) と局所束縛を SCC に分けて依存順に推論し、シグネチャのない束縛を
  まとめて一般化する。シグネチャ付き束縛は剛性型変数で検査する (多相再帰可)。パターン束縛は単相。
  未解決のメタ変数は `()` に既定化する (型クラスがない H1 では曖昧性は生じない)。
- 種: データ型の引数の種は使われ方 (適用される引数の数) から求める。H1 では高階の種推論はこの近似で足りる。
- 型クラスの制約・`class` / `instance` は H2 (FEAT-0110) まで E0001。
- 構文の修正: `(>>=) = primBindIO` のような括弧付き演算子の変数束縛を受理するよう achc-syntax を修正。
- テスト: `crates/achc-typeck/src/tests.rs` (6 件)、`tests/achc/typeck/{ok,fail}` (推論結果 `.tc` と診断 `.stderr`)。

