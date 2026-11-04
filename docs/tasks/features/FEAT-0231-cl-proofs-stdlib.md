# FEAT-0231: 等式・証明と証明付き標準型

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0230
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

命題等価 `=`・`Refl`・`rewrite`・`with`、`Nat`/`Fin`/`Vect`/サイズ付き配列、消去の保証。

## 作業内容

- [ ] 等式型と rewrite
- [ ] with 抽象
- [ ] `core` の Nat / Fin / Vect / Dec / So
- [ ] 証明が消去されることの確認

## 受け入れ条件

- [ ] `append` の長さ・`reverse` の性質などの証明例が通る
- [ ] 証明付きコードと証明なしコードの生成コードが同一
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

