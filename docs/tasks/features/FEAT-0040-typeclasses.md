# FEAT-0040: 型クラスと deriving

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P0
- 依存: FEAT-0037
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型クラス・インスタンス・スーパークラス・デフォルトメソッド・既定化 (defaulting)、辞書渡しへの elaboration、deriving。

## 作業内容

- [ ] 制約の生成と解決
- [ ] 辞書渡し
- [ ] Haskell 2010 の defaulting 規則
- [ ] deriving (Eq, Ord, Show, Read, Enum, Bounded, Ix)
- [ ] Functor/Monad による do 記法

## 受け入れ条件

- [ ] 型クラスの run テストと型エラーテスト
- [ ] deriving Show/Read の出力が GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

