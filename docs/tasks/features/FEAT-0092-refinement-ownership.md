# FEAT-0092: 洗練型と所有権・借用の統合

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P2
- 依存: FEAT-0091, FEAT-0053
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

一意配列・借用に対する measure (`len`) と、消費前後の値の関係を述べる洗練 (Flux / Verus 方式)。

## 作業内容

- [ ] LType 値の消費前後の関係を述べる洗練
- [ ] `Ref r` / `Borrow r` 越しの measure
- [ ] `Cirka.Array` の主要関数に洗練型を付与

## 受け入れ条件

- [ ] in-place ソートの境界安全性が注釈のみで検証できる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

