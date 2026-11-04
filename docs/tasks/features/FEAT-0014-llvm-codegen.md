# FEAT-0014: LLVM IR 生成とリンク

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0010, FEAT-0013
- 設計: [07-backend.md](../../design/07-backend.md)

## 概要

MIR から LLVM IR テキストを生成し、clang でオブジェクト化してランタイムとリンクする。

## 作業内容

- [ ] LLVM IR テキストビルダ (opaque pointer)
- [ ] 命令の変換 (dup/drop/alloc の高速経路はインライン)
- [ ] `clang` 呼び出しとリンク
- [ ] 開発用コマンド `cirka-mir run foo.mir`

## 受け入れ条件

- [ ] `tests/mir/*.mir` がネイティブ実行でインタプリタと同じ出力・同じ確保/解放回数
- [ ] 生成 IR が `opt -passes=verify` を通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

