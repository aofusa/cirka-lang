# BUG-0004: 変数を含む列の網羅性検査が誤って W0610 を出す

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0110 (`Semigroup (Maybe a)` のインスタンス)
- 関連: FEAT-0104

## 現象

次の定義は網羅的なのに `warning[W0610]: non-exhaustive patterns` が出る。

```haskell
f :: Maybe a -> Maybe a -> Int
f Nothing b = 0
f a Nothing = 1
f (Just a) (Just b) = 2
```

## 再現手順

```sh
achc check bug.hs   # 上の定義を含むファイル
```

## 期待される動作

警告を出さない (GHC も出さない)。

## 原因

<未調査。第 1 列が変数の行 (`f a Nothing`) を、第 1 列のコンストラクタ `Just` の行列へ特殊化するときに落としている可能性>

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ

`Cirka.Base` では最初の列にも `Just a` と書いて回避している。
