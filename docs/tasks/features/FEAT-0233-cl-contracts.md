# FEAT-0233: 契約 (requires / ensures / invariant)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.4 証明と検証
- 優先度: P1
- 依存: FEAT-0232
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

契約構文を部分集合型に脱糖し、`--contracts=off|check|verify` を実装する。

## 作業内容

- [ ] 構文と脱糖
- [ ] check モードの実行時検査
- [ ] off で生成コードが同一であることの確認

## 受け入れ条件

- [ ] 二分探索・不変条件付きデータ型の例が verify で pass
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

