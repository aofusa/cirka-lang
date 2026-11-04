# FEAT-0142: DataKinds・PolyKinds・型演算子

- 状態: todo
- トラック: aCHC
- マイルストーン: H5 高度な型システム (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0140
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

構築子の昇格、型レベルのリスト・タプル、種多相、TypeOperators、ConstraintKinds。

## 作業内容

- [ ] 昇格と種検査 (LType は昇格しない)
- [ ] 種多相
- [ ] TypeOperators / ConstraintKinds

## 受け入れ条件

- [ ] 型レベルリストを使う例 (HList 等) のテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

