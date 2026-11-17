# BUG-0029: 検査器が配列を作る繰り返しを評価して指数時間・メモリを使う

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0254 (`tests/cirkalang/run/v006/sized_arrays.cirka`)
- 関連: FEAT-0244, FEAT-0254

## 現象

一意な配列を繰り返しで埋め、その大きさを部分集合型が読むプログラム (`let sq = U.freezeS (squares 8 (U.zerosS 8) 0)` の後の
`U.index sq 7`) の `cirka check --contracts=check` が 76 秒、4.8 GB のメモリを使った (`--contracts=off` は 0.9 秒)。
時間は配列の大きさについて指数的に増える (大きさ 4 で 0.2 秒、8 で 39 秒)。`verify` (Z3 あり) でも同じ評価を先に行う。

## 再現手順

```sh
cirka check --contracts=check tests/cirkalang/run/v006/sized_arrays.cirka
```

## 期待される動作

配列を作る繰り返しはコンパイル時に評価されず、検査は 1 秒未満で終わる。

## 原因

検査器が、値の分からない式を評価して調べる 2 か所で、配列を作る繰り返しを走らせていた。評価器の値は環境の複製を持つので
(止まった `case` が環境を `to_vec` で写す)、止まった式が入れ子になると値の大きさが指数的に増える。

1. 評価器 (`cirkalang-elab` の `prim_reduce`) は、プリミティブの適用を簡約するとき、簡約できるプリミティブ (整数の演算・比較など) か
   どうかを見る前にすべての引数を評価していた。`primUArraySize (freezeS (squares 8 …))` は簡約できないのに、引数として繰り返しを評価した。
   義務の「評価で決める」段階 (spec-v0.0.4.md §3.2 の 1) がこれを通る。
2. 検査器の歩き (`cirkalang-check` の `Walker::is_dict`) は、トレイトの辞書を受け取る関数の呼び出しで、各引数が辞書かどうかを
   評価して調べていた。辞書になりえない式 (`toList sq` の `sq`、`size sq - 1`) も評価した。

## 修正

- [x] 回帰テストを追加 (`cirkalang-elab` の `stuck_primitives_do_not_evaluate_their_arguments`: 簡約できないプリミティブの引数の繰り返しを
      評価しない (燃料を使わない) こと。直す前は 75 秒かかって失敗することを確かめた。2 つ目の原因は、FEAT-0254 の
      `cirkalang_sized_arrays_remove_checks` (`tests/cirkalang.rs`) が `sized_arrays.cirka` のコンパイルを 30 秒以内と確かめる)
- [x] 修正: `prim_reduce` は簡約できるプリミティブだけ引数を評価する。`is_dict` は、関数の結果の型がトレイト (または型を計算する定義) の
      呼び出しだけを評価する (型変数の結果は辞書にならない: 推論器は辞書をトレイトの型の位置にだけ渡す)

## メモ

- `sized_arrays.cirka` の検査は 0.14 秒、17 MB になった。
- 値が環境を共有しない (指数的に大きくなりうる) ことそのものは直していない。評価器の値を `Rc` で共有する変更は大きいので、
  別の場面で現れたら起票する。
