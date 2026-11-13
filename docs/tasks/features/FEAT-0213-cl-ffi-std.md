# FEAT-0213: FFI と libc ベースの std (CUI 向け)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0212
- 設計: [08-ffi.md](../../design/08-ffi.md), [09-libraries.md](../../design/09-libraries.md)、[spec-v0.0.2.md](../../design/cirkalang/spec-v0.0.2.md) §8、ADR-069, 070

## 概要

`extern "C"` (所有権付きポインタ)、std の最小部: ファイル、標準入出力、引数、環境変数、プロセス終了、時刻。

## 作業内容

- [x] extern 構文と C ABI 型の検査 (`extern "C"` / `extern "cirka"`、E1007 / E1270 / E1271。Core のラッパと C シグネチャ付きの `extern` に下ろす)
- [x] `Foreign` (core): C の型・`Ptr` / `OwnedPtr`・`withCString` / `peekCString`・メモリの読み書き
- [x] `lib/cirkalang/std` の fs / io / env / process / time (`Std.Fs` `Std.IO` `Std.Env` `Std.Process` `Std.Time`)。ランタイムの `stdio.rs` とインタプリタの実装
- [x] Prelude に `print` `isJust` `isNothing`

## 受け入れ条件

- [x] ファイルを読んで書く CUI ツールの run テスト (`run/v002/tool_wc`、`std_fs`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

テスト: `run/v002/{ffi_libc,std_fs,std_stdin,std_env,tool_wc}` (インタプリタ -O0/-O2 とネイティブで一致、リーク 0)、`fail/type/{extern_type,extern_conflict}`、`fail/syntax/extern_abi`、`parse/extern`。

範囲外として残したもの: 構造体の値渡し・コールバック・可変長引数・任意のライブラリのリンク (FEAT-0047)、bindgen (FEAT-0048)、`BorrowedPtr` (借用の領域)。
ファイルの書き込みは文字列を 1 回の `write` ループで渡す (複製なし)。`withCString` は NUL 終端のために 1 回複製する。

