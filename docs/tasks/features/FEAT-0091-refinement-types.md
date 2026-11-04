# FEAT-0091: 洗練型 (Liquid Haskell 互換注釈 + .chs 構文)

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P1
- 依存: FEAT-0090
- 設計: [13-verification.md](../../design/13-verification.md), [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

`{-@ @-}` 注釈 (Liquid Haskell のサブセット) と `CirkaRefinement` のネイティブ構文を解析し、部分型付けの VC を生成する。

## 作業内容

- [ ] `{-@ type / measure / predicate / シグネチャ @-}` の解析
- [ ] `{v : τ | p}` と `(x : τ) ->` 構文 (CirkaRefinement)
- [ ] 述語の論理 (線形整数算術・真偽値・未解釈関数・measure)
- [ ] 部分型付け → VC
- [ ] 遅延束縛の扱い (13 §3.5)

## 受け入れ条件

- [ ] safeDiv・範囲内 index・非空リストの例が pass、違反が反例付きで fail
- [ ] 注釈を消した .hs が GHC でもコンパイルできる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

