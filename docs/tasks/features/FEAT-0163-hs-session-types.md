# FEAT-0163: セッション型 (Cirka.Session)

- 状態: todo
- トラック: aCHC
- マイルストーン: H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後)
- 優先度: P2
- 依存: FEAT-0162, FEAT-0122, FEAT-0262
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

線形チャネル + GADTs + DataKinds によるセッション型ライブラリ。

## 作業内容

- [ ] Send / Recv / Offer / Choose / End と Dual
- [ ] 例題

## 受け入れ条件

- [ ] プロトコル違反・途中放棄が型エラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

