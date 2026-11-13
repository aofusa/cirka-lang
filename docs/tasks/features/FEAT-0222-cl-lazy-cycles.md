# FEAT-0222: 明示的な遅延 (Lazy a) と循環の扱い

- 状態: in-progress
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P1
- 依存: FEAT-0215
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

`Lazy a` / `delay` / `force`。正格言語で循環が生じうる箇所 (Lazy の自己参照・可変参照) の静的警告。

## 作業内容

- [ ] Lazy
- [ ] 循環の静的検出
- [ ] 無限ストリームの例

## 受け入れ条件

- [ ] 無限ストリームの例がリーク 0 で動く
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

