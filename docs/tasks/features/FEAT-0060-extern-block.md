# FEAT-0060: extern "C" ブロックと所有権付きポインタ

- 状態: todo
- マイルストーン: M6 FFI 拡張
- 優先度: P1
- 依存: FEAT-0046, FEAT-0052
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`extern "C" link ... { }` 構文、`OwnedPtr`/`BorrowedPtr`/`Nullable`、`free with` 属性による Release 自動生成。

## 作業内容

- [ ] 構文と脱糖
- [ ] C ABI で表現可能な型の検査
- [ ] BorrowedPtr への r の補完と BorrowIO への持ち上げ
- [ ] `free with`

## 受け入れ条件

- [ ] `FILE*` を RAII で扱う例が動き、閉じ忘れが型エラーになる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

