# FEAT-0151: 洗練型 (Liquid Haskell 互換注釈 + .chs 構文)

- 状態: todo
- トラック: aCHC
- マイルストーン: H6 依存型と検証 (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0050, FEAT-0105, FEAT-0235
- 設計: [14-achc-advanced.md](../../design/14-achc-advanced.md), [13-verification.md](../../design/13-verification.md)

## 概要

`{-@ @-}` 注釈と `CirkaRefinement` 構文を Core 上の独立パスで検証する (cirka-smt を使用)。

## 作業内容

- [ ] 注釈の解析 (type / measure / predicate / シグネチャ)
- [ ] `.chs` のネイティブ構文
- [ ] VC 生成と SMT
- [ ] 遅延束縛の扱い
- [ ] `achc verify` と反例の表示

## 受け入れ条件

- [ ] safeDiv・範囲内 index・非空リストの例が pass/fail する
- [ ] 注釈を消した .hs が GHC でもコンパイルできる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

