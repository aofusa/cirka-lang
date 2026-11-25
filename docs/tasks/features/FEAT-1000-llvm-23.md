# FEAT-1000: バックエンドの LLVM を Rust と同じ 23 に上げる

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0091
- 設計: [07-backend.md](../../design/07-backend.md) §1, [00-overview.md](../../design/00-overview.md) ADR-110

## 概要

生成した LLVM IR は clang-18 (Ubuntu 24.04 の apt パッケージ) で最適化して機械語にしている。比較の相手の Rust (rustc 1.99.1) は
LLVM 23.1 を使う。n-body は FEAT-0098 でループの中の検査の分岐をなくしても Rust の 1.16 倍のままだった。Rust の n-body の最適化前の
IR を clang-18 でコンパイルすると、SLP ベクトル化が平方根と割り算を 2 組ずつ残し、こちらと同じ機械語になる (LLVM 23 ではすべて組になる)。
Ivy Bridge の除算器はパイプライン化されておらず、平方根と割り算の命令の数がステップの時間を決める。差はフロントエンドではなく LLVM の版にある。

バックエンドの LLVM を、apt.llvm.org の Ubuntu 24.04 向けパッケージの 23 に上げる (ADR-110)。生成する IR は版に依らない形なので変えない。

## 作業内容

- [x] 実験: 全ベンチの IR を clang-23 でコンパイルし (使い捨てのコンテナに clang-23 を入れる)、出力の一致と時間を確かめる
- [x] Dockerfile: LLVM を apt.llvm.org から入れる層を最後に分け、版を変えてもその層だけを作り直す。既定の版を 23 にする
- [x] `.env.sample`・compose.yaml・`cirka-bindgen` の既定・設計書 (07-backend.md・ADR-110)・README・AGENTS.md の版を 23 にする
- [x] イメージを作り直し、`scripts/ci.sh` (TSan・bindgen を含む) を通す
- [ ] 全ベンチと HTTP サーバを計測して記録する

## 受け入れ条件

- [x] 全テスト (TSan・bindgen を含む) が LLVM 23 で通る
- [ ] 全ベンチの出力が変わらない
- [ ] n-body が速くなる

## メモ

- clang-23: 23.1.6 (apt.llvm.org の `llvm-toolchain-noble-23`、2026-11-16 の版)。rustc 1.99.1 の LLVM は 23.1.1。
- 実験 (2026-11-25): FEAT-0098/0099 の後に cirka が出した全ベンチの LLVM IR を、使い捨てのコンテナに入れた clang-23 で
  ドライバと同じ引数でコンパイルし直した。出力はすべて同じ。11 回の交互実行の中央値 (clang-18 に対する比 / Rust に対する比):

  | 題材 | clang-23 / clang-18 | clang-23 / Rust |
  |---|---:|---:|
  | array-sort | 1.026 | 1.080 |
  | binary-search | 1.034 | 1.219 |
  | binary-trees | 0.999 | 0.464 |
  | cli-grep | 1.003 | 1.027 |
  | mandelbrot | 1.000 | 0.988 |
  | n-body | 0.855 | 0.970 |
  | spectral-norm | 1.002 | 1.014 |
  | stencil | 0.984 | 0.993 |
  | string-split | 1.024 | 1.073 |
  | effects | 0.927 | 0.950 |
  | session | 0.998 | 0.216 |

  n-body のステップのループは `sqrtpd` 5・`divpd` 5 になった (Rust と同じ)。array-sort・binary-search・string-split の遅れは
  LLVM の版によらず Rust より遅い題材で、FEAT-0091 で続けて調べる。
- イメージ: 3.72 GB (LLVM 18 のイメージは 3.59 GB)。コンテナの中の TSan の確認 (`clang-23 -fsanitize=thread`) は通る。
- CI (2026-11-25): 作り直したイメージで `scripts/ci.sh` の全段 (fmt・clippy・ランタイムの 3 種のビルド・TSan・Vulkan・BLAS/LAPACK・全テスト) が通る。
