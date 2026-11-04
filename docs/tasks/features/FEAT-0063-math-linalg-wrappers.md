# FEAT-0063: cirka-math / cirka-linalg 安全ラッパーの試作

- 状態: todo
- マイルストーン: M6 FFI 拡張
- 優先度: P2
- 依存: FEAT-0060, FEAT-0053
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

specs 02 の方針 (C の成熟した資産を安全な薄いラッパーで使う) を第一段階で試作する。libm と OpenBLAS/LAPACKE のラッパー。

## 作業内容

- [ ] Dockerfile に `libopenblas-dev` / `liblapacke-dev` を追加
- [ ] `cirka-math`: libm の純粋関数ラッパー
- [ ] `cirka-linalg`: `Matrix` (行優先/列優先を明示)、`gemm` (純粋版) と `gemmIn` (一意な出力行列への in-place 版)
- [ ] BLAS 呼び出しと手書きループの性能比較

## 受け入れ条件

- [ ] 行列積・連立一次方程式の例が動き、結果が参照実装と一致
- [ ] in-place 版で余分な確保がないことを確保回数で確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

