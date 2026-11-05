# BUG-0008: newtype の構築子パターンが値を評価してしまう

- 状態: todo
- 重大度: major
- 発見箇所: FEAT-0119 (互換計測、GHC testsuite の codeGen/cgrun062)
- 関連: FEAT-0104, FEAT-0119

## 現象

```haskell
newtype T = C { f :: String }
main = print (case undefined of C {} -> True)
```

GHC は `True` を出力するが、aCHC は `Prelude.undefined` で終了する。

## 再現手順

```sh
scripts/compat.sh   # CIRKA_COMPAT_FILTER=codeGen/cgrun062
```

## 期待される動作

Haskell 2010 §4.2.3: newtype の構築子に対する照合は値を評価しない (パターンは中のパターンの照合と同じ)。
`case undefined of C _ -> True` は `True`、`case undefined of C x -> ...` は `x` が使われるまで評価しない。

## 原因

脱糖 (パターン照合のコンパイル) が newtype の構築子パターンを通常の構築子と同じく `case` で照合しており、
newtype は表現が中身と同じなので `case` が中身を評価する。

## 修正

- [ ] 回帰テストを追加
- [ ] newtype の構築子パターンを、照合する値のキャストに対する中のパターンの照合として脱糖する

## メモ
