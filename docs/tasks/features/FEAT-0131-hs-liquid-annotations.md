# FEAT-0131: Liquid Haskell 互換注釈の検証 (任意)

- 状態: todo
- トラック: aCHC
- マイルストーン: H4 FFI 拡張と評価
- 優先度: P2
- 依存: FEAT-0050, FEAT-0105
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

`{-@ @-}` 注釈 (サブセット) を cirka-smt で検証する。CirkaLang 側の検証を優先し、余力があれば行う。

## 作業内容

- [ ] 注釈の解析
- [ ] VC 生成
- [ ] `achc verify`

## 受け入れ条件

- [ ] safeDiv・範囲内 index の例が pass/fail する
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

