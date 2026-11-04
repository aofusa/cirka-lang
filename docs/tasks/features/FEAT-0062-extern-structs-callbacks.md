# FEAT-0062: C 構造体とコールバック

- 状態: todo
- マイルストーン: M6 FFI 拡張
- 優先度: P2
- 依存: FEAT-0060
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`struct` 宣言 (C レイアウト)、値渡し (SysV ABI)、可変長引数、`mkCallback`。

## 作業内容

- [ ] 構造体のレイアウト計算と値渡し
- [ ] 可変長引数
- [ ] コールバックのトランポリン

## 受け入れ条件

- [ ] qsort にコールバックを渡す例、clock_gettime の例
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

