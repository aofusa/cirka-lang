# FEAT-0060: ランタイムの高速経路のインライン化 (bitcode と LTO)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: FEAT-0014, FEAT-0051
- 設計: [07-backend.md](../../design/07-backend.md) §4、[16-performance.md](../../design/16-performance.md) §4

## 概要

`libcirka_rt.a` の関数 (確保の遅い経路・`cirka_force`・文字列や配列のプリミティブ) は生成コードから不透明な呼び出しになる。
ランタイムを LLVM bitcode でも出力し、生成した IR とリンク時に最適化して、小さな関数をインライン化する。

## 作業内容

- [ ] `cirka-runtime` を bitcode でも出力する (Rust と clang の LLVM の版の差を確認する)
- [ ] `-O2` で bitcode と一緒に最適化する経路をドライバに加える (版が合わなければ従来の静的リンク)
- [ ] ベンチで前後を計測する

## 受け入れ条件

- [ ] 全実行テストがリーク 0 のまま通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
