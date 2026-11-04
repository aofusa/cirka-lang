# FEAT-0232: 洗練型 (部分集合型) と SMT による自動証明

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P0
- 依存: FEAT-0231, FEAT-0050
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

`{x : A | p}` を「値 + 消去される証明」として扱い、証明義務を cirka-smt で自動的に解く。

## 作業内容

- [ ] 部分集合型の構文と elaboration
- [ ] 証明義務の SMT 問い合わせへの変換
- [ ] 反例の表示 (E0501)
- [ ] 解けない場合の手動証明へのフォールバック

## 受け入れ条件

- [ ] 範囲内 index・非ゼロ除算・ソート済みの例が自動で通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

