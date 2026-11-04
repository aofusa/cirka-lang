# FEAT-0032: 共通ドライバ (外部ツール・中間生成物・ダンプ)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P0
- 依存: FEAT-0014, FEAT-0003
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

両 CLI (`achc` / `cirka`) が共有するパイプライン実行部。

## 作業内容

- [x] Core 以降のパイプライン実行
- [x] `_build/` の管理
- [x] `--dump=core,core-opt,mir,mir-opt,llvm`
- [x] `-O0/-O1/-O2`、`--runtime=release|debug`

## 受け入れ条件

- [x] Core を入力とする統合テストが両 CLI から同じ結果になる
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ


- `cirka-driver::pipeline::compile_core` が Core Lint → Core 最適化 (-O1 以上) → Core→MIR → Perceus → MIR 検証器 → LLVM IR を実行し、`link` が clang + ランタイムでリンクする。`.core` 入力の Lint エラーは利用者の誤り (診断)、フロントエンド出力の Lint エラーは ICE。
- `cirka-driver::cli::main` が `build` / `run` / `check`、`-O0/1/2`、`--runtime=`、`--dump=` (`all` 可)、`--build-dir=`、`--emit=llvm`、`-o`、`-l`/`-L` を処理する。`achc` / `cirka` はフロントエンドを渡すだけ (現時点では `.core` のみ)。
- 統合テスト `tests/driver.rs`: tests/core/run の全プログラムを両 CLI × (-O0 debug / -O2 release) で実行し、出力と終了コードが一致することを確認。`check` の診断とダンプも確認。
