# FEAT-0014: LLVM IR 生成とリンク

- 状態: done
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P0
- 依存: FEAT-0010, FEAT-0013
- 設計: [07-backend.md](../../design/07-backend.md)

## 概要

MIR から LLVM IR テキストを生成し、clang でオブジェクト化してランタイムとリンクする。

## 作業内容

- [x] LLVM IR テキストビルダ (opaque pointer)
- [x] 命令の変換 (dup/drop/alloc の高速経路はインライン)
- [x] `clang` 呼び出しとリンク
- [x] 開発用コマンド `cirka-mir run foo.mir`

## 受け入れ条件

- [x] `tests/mir/*.mir` がネイティブ実行でインタプリタと同じ出力・同じ確保/解放回数
- [x] 生成 IR が `opt -passes=verify` を通る
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- MIR の関数はすべて `tailcc`。関数名は `_C<長さ><名前>` でマングルする。`let r = call/apply ...; ret r` も `musttail` にする。
- dup/drop の高速経路は release ランタイム向けにインライン展開し、debug ランタイム向けは検査付きのランタイム関数を呼ぶ (`Options::debug_rt`)。
- 開発用コマンドは `cirka-driver` の bin `cirka-mir` (`check` / `interp` / `run` / `emit-llvm`)。
- `tests/mir.rs` の `mir_run_native` が、全 `tests/mir/run/*.mir` について debug / release の両ランタイムでネイティブ実行し、インタプリタと出力・終了コード・確保/解放回数が一致すること、`opt -passes=verify` を通ること、リーク報告がないことを確認する。
