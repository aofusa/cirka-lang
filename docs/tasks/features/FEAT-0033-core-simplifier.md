# FEAT-0033: Core 簡約器とインライン

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P1
- 依存: FEAT-0030
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

β 簡約・case-of-known-constructor・let 浮動・インライン・DCE。各パス後に Core Lint。

## 作業内容

- [ ] 簡約器
- [ ] インライン判定 (多重度 1 の情報を利用)
- [ ] 各パス後の Lint

## 受け入れ条件

- [ ] 最適化の前後で実行結果が変わらない
- [ ] 確保回数の削減を計測
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

