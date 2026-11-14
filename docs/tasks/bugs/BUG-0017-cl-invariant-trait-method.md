# BUG-0017: トレイトのメソッドを使う不変条件が check で ICE になり、W1231 が誤って出る

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0235 (検証済みサンプルのソート)
- 関連: FEAT-0233

## 現象

`invariant a <= b && b <= c` のように `Ord` の `<=` と `&&` を使う不変条件を持つデータ型で、

- `--contracts=check` (および Z3 で証明できなかった述語の実行時検査) で
  `internal compiler error: Core Lint after lowering: ... constructor Prelude.Ord$Dict does not belong to type Prelude.Bool` になる。
- どの検査方式でも `warning[W1231]: unreachable alternative` が不変条件の位置に出る。

## 再現手順

```sh
cirka run --contracts=check tests/cirkalang/contracts/invariant_ord.cirka
```

## 期待される動作

ICE にならず、不変条件が実行時に検査される。W1231 は出ない。

## 原因

- 実行時検査に置き換えるとき、述語の中の変数でない検査対象を `Bool` 型の `let` に束縛していた (lowering は変数への `case` しか扱わない)。述語にはインスタンスの辞書 (大域定義) への `case` も含まれ、これも `Bool` として束縛していた。
- 精密化を含む構築子の型は、データ型の宣言の時点と、データ型が揃った後 (`refined_cons`) の 2 回エラボレートされる。1 回目では述語の `&&` の場合分けの網羅性が正しく求まらず、W1231 が出ていた。

## 修正

- [x] 回帰テストを追加 (`cirkalang_invariant_with_trait_methods`)
- [x] 修正: 真偽値への `case` は `Bool` で、大域定義への `case` はその大域定義の型で束縛する。精密化を含む構築子の 1 回目のエラボレートでは W1231 を捨てる (2 回目で検査する)。

## メモ
