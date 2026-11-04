# FEAT-0160: ケイパビリティ (Cirka.Capability)

- 状態: todo
- トラック: aCHC
- マイルストーン: H7 効果・能力・セッション型 (CirkaLang v0.0.7 の後)
- 優先度: P2
- 依存: FEAT-0263, FEAT-0113
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md)

## 概要

能力トークンと capability-safe モジュール検査。CirkaLang v0.0.7 の設計に合わせる。

## 作業内容

- [ ] `Cap e` と取得方法
- [ ] 能力付き IO ラッパ
- [ ] capability-safe 検査

## 受け入れ条件

- [ ] 能力のない関数がファイルを読めない (型エラー)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

