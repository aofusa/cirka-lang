# FEAT-0047: FFI ABI の完成 (構造体・共用体・可変長引数・コールバック)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [08-ffi.md](../../design/08-ffi.md) §7.2、ADR-092

## 概要

x86_64 SysV ABI に従った構造体/共用体の値渡し、アラインメント・packed、可変長引数、コールバックのトランポリン。Vulkan 等に必要。

## 作業内容

- [x] 構造体/共用体のレイアウト計算と値渡し (`cirka_core::ctype`、Core の `cstruct`/`cunion`、MIR の `cstruct`/`cunion`)
- [x] 可変長引数 (`...` とリンク名 `:link`)
- [x] コールバックのトランポリン (ランタイムの 4096 個のスタブ `callback.rs` と、バックエンドが作る `nest` のトランポリン)
- [x] 関数ポインタの呼び出し (`:dynamic`) とシンボルのアドレス (`:addr`)
- [x] errno の取得 (インタプリタも C の呼び出しの前後で errno を受け渡す。aCHC は `Foreign.C.Error`)
- [x] aCHC: `foreign import ccall "&sym"` / `"dynamic"` / `"wrapper"`、`freeHaskellFunPtr`

## 受け入れ条件

- [x] qsort へのコールバック、clock_gettime、構造体の値渡しのテスト
  (`tests/core/run/c_structs`・`c_varargs`・`c_callbacks`、`tests/achc/run/h2/ffi_callbacks.hs`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計 (08-ffi.md §7.2、ADR-092): 集成型の値は Core ではアドレス (`Int#`) で表し、バックエンドが SysV の分類で読み書きする。
  可変長引数は `...` とリンク名、関数ポインタの呼び出しは `:dynamic`、シンボルのアドレスは `:addr`、コールバックは `:callback h`
  (ランタイムの固定個のスタブが環境を `r10` に入れてトランポリンへ跳ぶ)。インタプリタはコールバック以外を扱う。
- 結果: 構造体は整数/SSE レジスタ・`byval`・`sret`・packed・共用体・レジスタ不足のすべてで、C コンパイラ (clang) が作った
  共有ライブラリとインタプリタ・ネイティブの両方で一致した (`c_structs`)。可変長引数は C ライブラリの `snprintf` と
  `va_arg` で読む補助関数で、スタックに溢れる整数・浮動小数・構造体の順序まで一致した (`c_varargs`)。コールバックは
  引数の全クラス・狭い整数・C → Haskell → C の入れ子・`qsort`・10000 回の作成と解放 (スロットの再利用) を、Core の最適化の
  前後でネイティブに確かめた (`c_callbacks`、リーク 0)。コールバックを使わないプログラムからはスタブと表がリンカで除かれる。
- コールバックの環境は共有オブジェクトにする (C がランタイムのどのスレッドから呼んでもよいように)。ランタイムが知らない
  スレッドからの呼び出しは扱わない (設計の制約)。
- 作業中に、最適化したランタイムの単体テストが SIGSEGV で落ちる既存の不具合を見つけた (BUG-0025、`siginfo_t` が 8 バイト短い)。

