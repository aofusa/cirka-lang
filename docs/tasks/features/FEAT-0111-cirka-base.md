# FEAT-0111: cirka-base: Prelude と基本モジュール (縮小版)

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0110
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

`lib/cirka-base` に Prelude と頻出モジュールだけを実装する (GHC base から移植可能なものは移植)。

## 作業内容

- [x] `Cirka.Prim` (stderr 出力 `primPutErrStr` を追加)
- [x] Prelude (FEAT-0110 で `Cirka.Base` + 再エクスポート。`Alternative`/`MonadPlus`、7 要素までのタプルのインスタンスを追加)
- [x] Data.List / Char / Maybe / Either / Function / Ord / Tuple / Foldable / Traversable / Functor、Control.Monad / Applicative、Text.Read、System.IO の主要部
  (`IORef` とファイル・ハンドル操作は FEAT-0113 で扱う)
- [x] 移植元のライセンス表記 (`lib/cirka-base/LICENSE-GHC-BASE`)

## 受け入れ条件

- [x] Prelude の全エクスポートが存在し run テストがある (`tests/achc/run/h2/prelude.hs`、stdin 付き)
- [x] 差分テストで GHC と一致 (`prelude.hs`・`base_modules.hs` の期待出力は Docker 内の GHC の出力と同一)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 途中で BUG-0004 (網羅性検査の誤警告) を修正した。
- 標準ライブラリが大きくなりビルドが遅くなった (hello world で 2.9 秒) ため、最適化の前と `-O0` でも
  `main` から到達しない大域定義を削除するようにした (0.6 秒)。

