# FEAT-0116: Haskell 2010 FFI (foreign import ccall)

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0113
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`foreign import ccall`、`Ptr`/`FunPtr`、Foreign.* の主要部分。

## 作業内容

- [x] 宣言の解析と型検査
- [x] MIR の `extern`
- [x] Storable と alloca/malloc/peek/poke

## 受け入れ条件

- [x] libc 関数を呼ぶ run テスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計は 08-ffi.md §7.1・ADR-035。共通 Core / MIR の `extern` に C シグネチャを追加し、LLVM バックエンドで変換、MIR インタプリタは `dlsym` で呼ぶ。
- ライブラリ: `Foreign`・`Foreign.{Ptr,Storable,C,C.Types,C.String,Marshal,Marshal.*}`・`Data.{Bits,Int,Word}`。
  `Cirka.Foreign.Types` は `scripts/gen-foreign-types.py` の生成物 (直接編集しない)。
- テスト: `tests/achc/run/h2/ffi.hs` (libc / libm の呼び出し、`Storable`、配列、UTF-8 の C 文字列、`Int8`〜`Word64`、`Bits`。GHC と一致)、
  `tests/achc/desugar/ffi_errors.hs` (E0618)、`tests/core/run/c_ffi.core`・`tests/core/fail/c_sig_mismatch.core`、
  `cirka-mir` の `ffi` / `c_signatures_round_trip_and_run`、`achc-desugar` の `ffi::tests`。
- 未対応 (FEAT-0047 で扱う): `&sym` の取り込み、`dynamic` / `wrapper` (コールバック)、構造体の値渡し、可変長引数、
  インタプリタでのスタック渡しの引数 (整数 7 個以上・浮動小数 9 個以上)。`foreign export` も未対応。
- `alloca` 系は例外時に解放しない (FEAT-0040 の後で `bracket` 相当にする)。`Integer` の `Bits` は未実装。

