# FEAT-0075: GHC 拡張の追加

- 状態: todo
- マイルストーン: M7 互換性と性能
- 優先度: P1
- 依存: FEAT-0040
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

02 §1.2 の拡張を互換レポートの優先度順に追加する。

## 作業内容

- [ ] RankNTypes, MPTC, FunctionalDependencies, FlexibleInstances/Contexts, GND, Deriving 系
- [ ] GADTs, TypeFamilies (後半)

## 受け入れ条件

- [ ] 各拡張の run / fail テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

