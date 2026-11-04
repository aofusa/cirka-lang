# FEAT-0206: プリミティブ型と最小ライブラリ

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0205
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [09-libraries.md](../../design/09-libraries.md)

## 概要

Int/UInt/Float/Bool/Char、算術、比較、最小の出力 (`println`)、`Unique` と `Drop` の最小限。

## 作業内容

- [ ] `lib/cirkalang/core` の最小部
- [ ] プリミティブ演算の Core への対応
- [ ] `Drop` の自動挿入 (最小)

## 受け入れ条件

- [ ] run テスト
- [ ] 所有権エラーの fail テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

