# FEAT-0111: cirka-base: Prelude と基本モジュール (縮小版)

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0110
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

`lib/cirka-base` に Prelude と頻出モジュールだけを実装する (GHC base から移植可能なものは移植)。

## 作業内容

- [ ] `Cirka.Prim`
- [ ] Prelude
- [ ] Data.List / Char / Maybe / Either / IORef、Control.Monad、System.IO の主要部
- [ ] 移植元のライセンス表記

## 受け入れ条件

- [ ] Prelude の全エクスポートが存在し run テストがある
- [ ] 差分テストで GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

