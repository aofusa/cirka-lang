# FEAT-0140: GADTs と存在型

- 状態: todo
- トラック: aCHC
- マイルストーン: H5 高度な型システム (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0132, FEAT-0235
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

構築子ごとの型の精密化と存在型。局所的な型等式制約 (OutsideIn(X) の簡易版)。Core では Any/cast で表す。

## 作業内容

- [ ] GADT 構文と型検査 (マッチには型注釈を要求)
- [ ] ExistentialQuantification
- [ ] 型等式の Core への落とし方 (Any/cast)

## 受け入れ条件

- [ ] 型付き式言語の評価器などの GADT 例が run テストで動く
- [ ] 型エラーテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

