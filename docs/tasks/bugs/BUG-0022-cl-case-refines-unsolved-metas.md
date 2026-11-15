# BUG-0022: 変数での場合分けが、その変数を引数に持つ未解決のメタ変数を解けなくする

- 状態: done
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

- [x] 回帰テストを追加 (`tests/cirkalang/run/v002/case_unknown_type.cirka`。期待型の分からない `case`、`Ptr a` と `List a` の局所変数、
      各枝のコンストラクタの 4 通り。修正前は 4 つとも E1200)
- [x] 置き換える前に、期待型と文脈の型の中の未解決のメタ変数から場合分けする変数を刈り込む (`prune` と同じ解き方)。
      `Ev::prune_var` が `?m … x …` を `λys. ?m' (x を除いた ys)` で解き、`Elab::prune_refined` が局所変数での場合分けの前に
      ゴールと文脈のすべての型に適用する
- [x] 仕様 (spec-v0.0.1.md §4.5 依存パターンマッチ) に規則を追記

## メモ

- FEAT-0242 のサンプルは、結果を文字列にする補助関数 (`describe`) などで避けて書いた。
- 既存のテストの診断は変わらない (cirkalang / mir / core / achc の統合テストがすべて通る)。ベンチ (array-sort, binary-trees, cli-grep,
  mandelbrot, n-body, spectral-norm, string-split) と http_echo / os_tool の最適化後の Core と LLVM IR は修正の前後で同一。
- 検討した別案: 期待型が分からないときに結果の型を先に決める (枝より先に `?a` を新しいメタ変数と単一化する)。文脈の局所変数の型
  (`buf : Ptr ?a`) の場合を直せないので採らなかった。
- 制約: 結果の型が分岐ごとに場合分けした変数に依存する `case` で、その依存が `case` の後で初めて分かるもの (期待型が後から決まる位置)
  は、メタ変数が変数に依存しないと決めるので型が合わなくなる。修正前もパターン単一化では解けなかった書き方で、期待型が分かる位置に書けば通る。
