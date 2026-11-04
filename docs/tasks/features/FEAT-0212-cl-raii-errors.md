# FEAT-0212: RAII (Drop/Release) とエラー処理

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0210
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

`Drop`/`Release` の自動挿入、`Result` と `?` 相当のエラー伝播、`panic` (abort。0040 後は unwind)。

## 作業内容

- [ ] Drop / Release
- [ ] Result とエラー伝播構文
- [ ] panic

## 受け入れ条件

- [ ] 解放順序の run テスト
- [ ] エラー伝播の run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

