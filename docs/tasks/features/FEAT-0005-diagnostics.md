# FEAT-0005: 診断基盤 (cirka-diagnostics)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

ソース位置と診断メッセージの共通基盤。両フロントエンドが使う。

## 作業内容

- [x] `FileId`, `Span`, `SourceMap`
- [x] `Diagnostic { code, severity, message, labels, notes, help }`
- [x] 端末向け表示とテスト用プレーン表示
- [x] ICE の報告形式

## 受け入れ条件

- [x] 複数ラベル・複数ファイルの診断がスナップショットどおり
- [x] UTF-8 (日本語含む) で列位置が正しい
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

