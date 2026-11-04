# FEAT-0213: FFI と libc ベースの std (CUI 向け)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0212
- 設計: [08-ffi.md](../../design/08-ffi.md), [09-libraries.md](../../design/09-libraries.md)

## 概要

`extern "C"` (所有権付きポインタ)、std の最小部: ファイル、標準入出力、引数、環境変数、プロセス終了、時刻。

## 作業内容

- [ ] extern 構文と C ABI 型の検査
- [ ] `lib/cirkalang/std` の fs / io / env / process / time

## 受け入れ条件

- [ ] ファイルを読んで書く CUI ツールの run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

