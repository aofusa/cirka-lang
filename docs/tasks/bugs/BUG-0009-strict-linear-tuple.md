# BUG-0009: タプルの成分の線形専用値が正格に評価されない

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0123 (in-place ソートのベンチ `bench/array-sort/own.chs`)
- 関連: FEAT-0122, FEAT-0123

## 現象

`(A.swap u i j, Ur i)` のようにタプルの成分に線形専用の型 (`Unique (Array e)`) の式を書くと、成分がサンクになり、
サンクが配列への 2 つ目の参照を持つ。サンクの評価中の `write` は配列が共有されていると判断してコピーする。
in-place ソートで 10 万要素あたり 6 万回以上のコピーが起きていた (`CIRKA_STATS` の `array_copies`)。

## 再現手順

```sh
CIRKA_STATS=1 achc run tests/achc/run/h3/strict_linear_tuple.hs   # array_copies=5
```

## 期待される動作

Strict-Linear (03-type-system.md §6) により、線形専用の型の構築子引数はタプルでも構築前に評価され、コピーは起きない。

## 原因

型推論で構築子・関数の引数には印 (`tast::Expr::Mark`) を付けていたが、タプル式 (`ExprKind::Tuple`) の成分には付けていなかった。
Strict-Linear の引数の正格化 (`raii.rs`) は印の付いた引数だけを対象にする。

## 修正

- [x] 回帰テストを追加 (`tests/achc/run/h3/strict_linear_tuple.hs`、`-- max-array-copies: 0`)
- [x] 修正 (タプルの成分にも印を付ける)

## メモ
