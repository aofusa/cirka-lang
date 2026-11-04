# FEAT-0242: OS 層 (syscall・シグナル・プロセス)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P1
- 依存: FEAT-0213
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

libc 経由の syscall ラッパ、errno → Result、シグナル、子プロセス、mmap。

## 作業内容

- [ ] std.os
- [ ] errno の Result 化
- [ ] シグナル・プロセス・mmap

## 受け入れ条件

- [ ] syscall を使う CUI ツールの run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

