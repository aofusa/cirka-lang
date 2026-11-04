# FEAT-0130: extern "C" ブロック (aCHC)

- 状態: todo
- トラック: aCHC
- マイルストーン: H4 FFI 拡張と評価
- 優先度: P1
- 依存: FEAT-0116, FEAT-0122, FEAT-0047
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`extern "C"` 構文、`OwnedPtr`/`BorrowedPtr`/`Nullable`、`free with`。

## 作業内容

- [ ] 構文と脱糖
- [ ] C ABI 型の検査
- [ ] BorrowedPtr への r の補完
- [ ] `free with`

## 受け入れ条件

- [ ] `FILE*` を RAII で扱う例が動き、閉じ忘れが型エラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

