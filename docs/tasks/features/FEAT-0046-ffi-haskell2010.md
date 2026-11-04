# FEAT-0046: Haskell 2010 FFI (foreign import ccall)

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P1
- 依存: FEAT-0043
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`foreign import ccall`、`Ptr`/`FunPtr`、Foreign.C.Types / Foreign.C.String / Foreign.Marshal の主要部分。

## 作業内容

- [ ] `foreign import ccall [unsafe] "header name"` の解析と型検査
- [ ] MIR の `extern` 命令
- [ ] Storable と alloca/malloc/peek/poke

## 受け入れ条件

- [ ] libc 関数 (strlen, qsort など) を呼ぶ run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

