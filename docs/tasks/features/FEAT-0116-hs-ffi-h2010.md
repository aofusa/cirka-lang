# FEAT-0116: Haskell 2010 FFI (foreign import ccall)

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0113
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`foreign import ccall`、`Ptr`/`FunPtr`、Foreign.* の主要部分。

## 作業内容

- [ ] 宣言の解析と型検査
- [ ] MIR の `extern`
- [ ] Storable と alloca/malloc/peek/poke

## 受け入れ条件

- [ ] libc 関数を呼ぶ run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

