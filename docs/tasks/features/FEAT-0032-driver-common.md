# FEAT-0032: 共通ドライバ (外部ツール・中間生成物・ダンプ)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P0
- 依存: FEAT-0014, FEAT-0003
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

両 CLI (`achc` / `cirka`) が共有するパイプライン実行部。

## 作業内容

- [ ] Core 以降のパイプライン実行
- [ ] `_build/` の管理
- [ ] `--dump=core,core-opt,mir,mir-opt,llvm`
- [ ] `-O0/-O1/-O2`、`--runtime=release|debug`

## 受け入れ条件

- [ ] Core を入力とする統合テストが両 CLI から同じ結果になる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

