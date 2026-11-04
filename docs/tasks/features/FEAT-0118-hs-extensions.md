# FEAT-0118: GHC 拡張 (初期分)

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P1
- 依存: FEAT-0110
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

差分テスト・cirka-base に必要な拡張だけを追加する (02 §1.2 の初期分)。GADTs・型族などの高度な型拡張は H5 (FEAT-0140〜)。

## 作業内容

- [ ] BangPatterns, ScopedTypeVariables, LambdaCase, TupleSections, MultiWayIf
- [ ] RankNTypes (注釈必須), MPTC, FlexibleInstances/Contexts, GND

## 受け入れ条件

- [ ] 各拡張の run / fail テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

