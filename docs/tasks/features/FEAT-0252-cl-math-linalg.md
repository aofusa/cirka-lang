# FEAT-0252: 数学・線形代数ラッパー

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P2
- 依存: FEAT-0250
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

specs 02 の方針で libm と OpenBLAS/LAPACKE の安全ラッパー。次元を型で表す。

## 作業内容

- [ ] Dockerfile に OpenBLAS/LAPACKE を追加
- [ ] std.math
- [ ] cirka-linalg (Matrix m n a、純粋版と in-place 版)

## 受け入れ条件

- [ ] 行列積・連立一次方程式の例が参照実装と一致
- [ ] 次元不一致が型エラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

