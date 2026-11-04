# FEAT-0152: 契約 (requires / ensures / invariant)

- 状態: todo
- トラック: aCHC
- マイルストーン: H6 依存型と検証 (CirkaLang v0.0.4 の後)
- 優先度: P2
- 依存: FEAT-0151
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

`.chs` の契約構文を洗練型に脱糖し、`--contracts=off|check|verify` を実装する。

## 作業内容

- [ ] 構文と脱糖
- [ ] check モードの実行時検査

## 受け入れ条件

- [ ] 二分探索の例が verify で pass
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

