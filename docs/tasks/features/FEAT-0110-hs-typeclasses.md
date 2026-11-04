# FEAT-0110: 型クラスと deriving

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0105
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型クラス・インスタンス・スーパークラス・デフォルトメソッド・defaulting、辞書渡し、deriving。

## 作業内容

- [ ] 制約の生成と解決
- [ ] 辞書渡し
- [ ] Haskell 2010 の defaulting
- [ ] deriving (Eq, Ord, Show, Read, Enum, Bounded)
- [ ] Monad による do 記法

## 受け入れ条件

- [ ] 型クラスの run / 型エラーテスト
- [ ] deriving Show/Read の出力が GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

