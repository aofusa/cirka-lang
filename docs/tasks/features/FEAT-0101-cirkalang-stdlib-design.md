# FEAT-0101: CirkaLang 標準ライブラリ設計

- 状態: todo
- マイルストーン: P2 CirkaLang
- 優先度: P2
- 依存: FEAT-0100
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

core / alloc / std の 3 層構成、glibc / libm / LLVM 組み込みの使い分けを確定する。

## 作業内容

- [ ] API 方針
- [ ] libc 依存範囲の決定
- [ ] 数学関数の実装方針

## 受け入れ条件

- [ ] 設計書がレビュー済み
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

