# FEAT-0005: 診断基盤 (cirka-diagnostics)

- 状態: todo
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: [01-architecture.md](../../design/01-architecture.md), [03-type-system.md](../../design/03-type-system.md)

## 概要

ソース位置と診断メッセージの共通基盤。以降のすべてのクレートが使う。

## 作業内容

- [ ] `FileId`, `Span`, `SourceMap` (行・列への変換)
- [ ] `Diagnostic { code, severity, message, labels, notes, help }`
- [ ] 端末向けの表示 (色付き、ソース抜粋) とテスト用のプレーン表示
- [ ] ICE (内部エラー) の報告形式

## 受け入れ条件

- [ ] 複数ラベル・複数ファイルの診断がスナップショットテストどおりに表示される
- [ ] UTF-8 (日本語を含む) ソースで列位置が正しい
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

