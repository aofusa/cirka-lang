# FEAT-0115: レコード構文とモジュールの完成

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0110
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

レコード、再エクスポート、複数モジュールのプロジェクト。

## 作業内容

- [x] レコードの脱糖
- [x] `module M` 再エクスポート
- [x] 複数モジュールのビルド

## 受け入れ条件

- [x] レコードと複数モジュールの run テスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- レコードの構築・更新・パターン・選択関数、`module M` の再エクスポート、修飾 import・`hiding`・`as` は FEAT-0102/0110 の時点で
  ほぼ動いていた。本チケットでは GHC との差を埋めた: 省いたフィールドの警告 (W0617)、正格フィールドの省略エラー (E0616)、
  どの構築子も持たないフィールドの組の更新エラー、実行時エラーの文言と位置 (`f.hs:5:10-20: Missing field ...` 等)。
- テスト: `tests/achc/run/h2/records/` (複数モジュール。`Main.hs` のあるディレクトリを一つのプログラムとして実行し、GHC には `-i` で渡す)、
  `run/h2/record_{missing,update,selector}.hs`、`typeck/fail/strict_field.hs`。
- GHC の `hPutStr` は文字列を内部の一時バッファに書いてから確定するので、文字列の途中で例外が起きるとその行の途中までは出力されない。
  aCHC は 1 文字ずつ出力するので途中まで出る。差分テストではこの状況を避ける (必要になれば別チケット)。
- 関数のパターン照合の失敗 (`Non-exhaustive patterns in function f`) はまだ GHC の文言と位置になっていない → BUG-0005。

