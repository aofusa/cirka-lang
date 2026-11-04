# FEAT-0210: トレイト (型クラス) と特殊化

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0207
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

一貫性のあるトレイト (孤児実装禁止)、辞書渡し + 特殊化、Eq/Ord/Show/Num 等の基本トレイト、導出。

## 作業内容

- [ ] trait / impl
- [ ] インスタンス解決 (一貫性検査)
- [ ] 辞書渡しと特殊化 (0041 と連携)
- [ ] derive

## 受け入れ条件

- [ ] トレイトの run / fail テスト
- [ ] 特殊化で辞書が消えることを確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

