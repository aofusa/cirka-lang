# BUG-0037: 引数の型が合わないと、その引数で決まる型の制約が E1254 として重ねて報告される

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0262 (`tests/cirkalang/fail/session/not_dual.cirka`)
- 関連: FEAT-0262

## 現象

関数の引数の型が合わない (E1200) と、その引数の型から決まるはずだった型変数が解けずに残り、
その型変数の制約が「どの実装を使うか決まらない」(E1254) として、同じ呼び出しについて重ねて報告される。

```
error[E1200]: type mismatch
26 |   (b, u) <- S.connect client both
   = note: expected `(1 c : Channel (dual ask)) -> IO _`
   = note:    found `(1 c : Channel (Recv Int (Recv Bool End))) -> IO Unit`
error[E1254]: cannot determine which instance of `Send _` to use
26 |   (b, u) <- S.connect client both
   = help: add a type annotation
```

`connect : Send b => ... -> ((1 c : Channel (dual s)) -> IO b) -> IO (a, b)` の `b` は 2 番目の引数の型から決まるが、
単一化が引数の型の途中で失敗して `b` が解けない。二つ目のエラーは一つ目の結果で、利用者が直すべき箇所ではない。

## 再現手順

```sh
cirka check tests/cirkalang/fail/session/not_dual.cirka
```

## 期待される動作

E1200 だけを報告する。型が合わなかった型の中の未解決のメタ変数は、領域の脱出 (E1200 の注記) と同じく失敗として印を付け、
その制約を重ねて報告しない。

## 原因

`unify_or_err` は、領域が脱出する場合だけ期待される型のメタ変数に失敗の印を付ける (`fail_metas`)。ほかの不一致では付けない。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ
