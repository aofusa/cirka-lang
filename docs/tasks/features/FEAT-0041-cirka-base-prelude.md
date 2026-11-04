# FEAT-0041: cirka-base: Prelude と基本モジュール

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P0
- 依存: FEAT-0040
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

`lib/cirka-base` に Prelude と頻出モジュールを実装する (GHC base から移植可能なものは移植)。

## 作業内容

- [ ] `Cirka.Prim` とコンパイラ組み込み
- [ ] Prelude 全体
- [ ] Data.List / Data.Char / Data.Maybe / Data.Either / Control.Monad / Data.Function 等
- [ ] 移植元のライセンス表記

## 受け入れ条件

- [ ] Prelude の全エクスポートが存在し、各関数に run テストがある
- [ ] 差分テストで GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

