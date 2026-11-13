# FEAT-0222: 明示的な遅延 (Lazy a) と循環の扱い

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P1
- 依存: FEAT-0215
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

`Lazy a` / `delay` / `force`。正格言語で循環が生じうる箇所 (Lazy の自己参照・可変参照) の静的警告。

## 作業内容

- [x] Lazy (`Data.Lazy`: `delay` `force` `now` `mapLazy`、Core の遅延フィールドの箱)
- [x] 循環の静的検出 (`Lazy` を含むトップレベルの値の警告 W0310。局所の `let` は再帰しない)
- [x] 無限ストリームの例 (`run/v003/lazy_stream`: 自然数・素数の篩・フィボナッチ)
- [x] `&&` / `||` の短絡評価 (`run/v003/short_circuit`)

## 受け入れ条件

- [x] 無限ストリームの例がリーク 0 で動く (インタプリタと debug ランタイムのリーク検査)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: spec-v0.0.3.md §4、ADR-074。`Lazy` のパラメータは `Type` なので、`Lazy` をフィールドに持つデータ型は `(a : Type)` と書く。
- `&&` の短絡は推論モードで `Bool` に対して検査する (`case` の結果の型を推論させると依存した型になるため)。
- W0310 は `cirkalang-elab` の単体テスト `top_level_lazy_values_warn`。
