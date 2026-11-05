# FEAT-0113: IO・IORef・ハンドル・例外

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0111
- 設計: [05-runtime.md](../../design/05-runtime.md)

## 概要

IO (State# 消去)、System.IO 主要関数、IORef、`error`/`exitWith` (abort)、`catch`/`bracket` は 0040 の後で。

## 作業内容

- [x] IO 型と State# の消去 (Core の newtype `IO a = Unit# -> a`、FEAT-0110 / ADR-032)
- [x] putStr/getLine/getContents/interact/readFile/writeFile (+ appendFile、isEOF、hFlush、stderr への出力)
- [x] IORef (ランタイムの MUTVAR)
- [x] System.Exit / Environment
- [x] `error` のメッセージ・終了コードを GHC と一致 (呼び出し位置の `CallStack` 行を含む)

## 受け入れ条件

- [x] `.stdin` 付き run テスト (`tests/achc/run/h2/io.hs`)
- [x] `error` 時の stderr と終了コードが GHC と一致 (`error.hs`・`file_error.hs`・`exit.hs`。run テストは `.stderr` があれば
  インタプリタの stderr と比べ、Docker では GHC の stderr がそれで始まることも確かめる)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 任意のハンドル (`openFile`/`hClose`/`hGetLine` …) と `catch` は FEAT-0040 (例外) の後。ファイルのエラーは今は終了するだけ。
- GHC の `head []` は base 内部の CallStack 行も出すが、aCHC は 1 行目 (メッセージ) だけを出す。

