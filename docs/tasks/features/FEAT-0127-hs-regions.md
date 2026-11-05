# FEAT-0127: リージョン (aCHC)

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P2
- 依存: FEAT-0124, FEAT-0049
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

`withRegion`/`RIO r`/`allocIn`/`readIn`/`newRefIn`。

## 作業内容

- [x] API (`Cirka.Region`: `withRegion`・`allocIn`・`readIn`・`newRefIn`・`readRefIn`・`writeRefIn`・`modifyRefIn`・`liftIO`)
- [x] 構文糖 `region r in e` (`CirkaRegions` / `CirkaOwnership`)
- [x] 脱出は E0320 (リージョン向けのメッセージ。`tests/achc/typeck/fail/region_escape.hs`)

## 受け入れ条件

- [x] リージョン内の循環グラフがリーク 0 (`tests/achc/run/h3/region.hs`: 1000 ノードの環)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

