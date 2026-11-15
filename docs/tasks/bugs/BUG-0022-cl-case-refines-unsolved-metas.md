# BUG-0022: 変数での場合分けが、その変数を引数に持つ未解決のメタ変数を解けなくする

- 状態: todo
- 重大度: major
- 発見箇所: FEAT-0242 (`tests/cirkalang/run/v005/os_tool.cirka` を書いているとき)
- 関連: FEAT-0203, FEAT-0222, FEAT-0242

## 現象

局所変数で場合分けする `case` の期待型や、ほかの局所変数の型に、まだ解けていないメタ変数があると、ふつうの (依存しない) コードが
E1200 になる。

```
orZero : Maybe Int -> IO Int
orZero m = pure (case m of
  Just n => n
  Nothing => 0)
```

```
error[E1200]: type mismatch
5 |   Just n => n
  = note: expected `_ (Just n)`
  = note:    found `Int`
```

局所変数の型の中のメタ変数でも起きる (`buf <- mallocBytes 16` の後の `case r of Just n => peekCStringLen buf 0` で
`expected Ptr CChar, found Ptr (_ (Just n))`)。単一化の一次近似が型の合わない解を作り、分かりにくい誤りになることもある
(`pure (case r of Just _ => Just s; Nothing => Nothing)` で `expected Just j0, found String`)。

## 再現手順

上の `orZero` を `cirka check` する。期待型が分かっている位置 (`"x" ++ (case …)`、`k <- case m of …` で枝が `IO Int`) では起きない。

## 期待される動作

期待型が分からない `case` は、枝の型から結果の型が決まる (依存しない場合分けとして扱う)。

## 原因

`elab_case` は場合分けする局所変数を各枝のパターンで置き換える (依存した場合分け)。メタ変数は作られた時点の束縛された変数すべてに
適用される (`?a Γ`) ので、変数を置き換えると `?a (Just n)` になり、パターン単一化 (`solve`。引数が相異なる変数のときだけ解く) で解けない。
解けないと一次近似 (`first_order`) が最後の引数を型の引数とみなして分解し、型の合わない解を作る。

## 修正

- [ ] 回帰テストを追加
- [ ] 置き換える前に、期待型と文脈の型の中の未解決のメタ変数から場合分けする変数を刈り込む (`prune` と同じ解き方)

## メモ

- FEAT-0242 のサンプルは、結果を文字列にする補助関数 (`describe`) などで避けて書いた。
