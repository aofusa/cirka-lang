# FEAT-0221: リージョン

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P1
- 依存: FEAT-0220, FEAT-0049
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

リージョン (アリーナ) 上の確保と一括解放。グラフ構造の表現。

## 作業内容

- [x] リージョン構文と型 (`Data.Region`: `withRegion`、`Arena r`、`RIO r`、`In r a`)
- [x] リージョン内可変参照 (`RefIn r a`: `newRefIn` `readRefIn` `writeRefIn` `modifyRefIn`)

## 受け入れ条件

- [x] グラフ構造の例がリーク 0 (`run/v003/region_graph`: 2 ノードの循環と 1000 ノードの環。インタプリタと debug ランタイムの両方でリーク検査)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: spec-v0.0.3.md §3、ADR-073。共通基盤のアリーナ (FEAT-0049) をそのまま使い、ランタイム・MIR には変更なし。
- 持ち出しは `fail/type/region_escape` (E1200、借用と共通のリージョンの注記)。
- リージョンを使うにはノードの型の添字のために `import Data.Borrow (Region)` も要る (モジュールは名前を再公開しない)。
