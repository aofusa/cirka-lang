# FEAT-0088: 場合分けの後の IO の動作をクロージャにしない

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0252
- 設計: [06-ir.md](../../design/06-ir.md), [16-performance.md](../../design/16-performance.md)

## 概要

FEAT-0252 の `tests/cirkalang/run/v006/linalg.cirka` を `-O2` でコンパイルすると、`Main.main` の中の
`gemm 1.0 a34 b42 1.0 (withOrder ColMajor c32)` で、`Linalg.gemmIO` の IO の動作 (3 つの行列の場合分けと、確保する大きさの `max` の分岐の後のラムダ) が
クロージャ (`Main.main$init$lam1$lam3`) として確保され、結合点の後で状態トークン (`$BoxI.B(0)`) に `apply` される。
クロージャが `c` の配列を持つので、`peekUArrayInto` の時点で配列が共有され、in-place にならずにコピーされる (`array_copies` が 1 増える)。

同じ `gemm` を関数 (`accumulate`) の中で呼ぶと、トークンの適用が場合分けと結合点の中に押し込まれて直線のコードになり、コピーは起きない。
単純化のラウンドの上限 `MAX_ITERS` を 6 から 40 にしても MIR は変わらないので、FEAT-0087 (収束の遅さ) とは別の原因である。
IO の関数を節 (パターン) で定義して純粋な文脈や大きな関数の中で呼ぶと、同じくクロージャの確保と間接呼び出しが起きると見られる。

## 作業内容

- [ ] 再現する最小の Core を作り、どの変形 (場合分け・結合点への引数の押し込み、`withOrder` の 2 つの腕の結合点) が効いていないかを調べる
- [ ] IO の関数を場合分けの後で呼ぶループのベンチを作り、確保数と時間を記録する (16-performance.md §4)
- [ ] 修正し、`linalg.cirka` の `max-array-copies` を 1 にする

## 受け入れ条件

- [ ] `linalg.cirka` の `gemm` がどれも in-place になる (`max-array-copies: 1`)
- [ ] ベンチの確保数が減り、ほかのベンチが遅くならない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 結果は正しく、コピーが 1 回増えるだけである。
- 見込み (未確認): `opt.rs` の `apply` は、`(case s of p -> e) a` を関数の型の `case` のときだけ腕に押し込み、`cast` を通す適用は
  `cast` の中がラムダか `let` のときだけ扱う。IO の動作は `Unit# -> a` を `IO a` に `cast` した値なので、`(cast (case …) (Unit# -> a)) token` が
  押し込まれずに残る可能性がある (`cast` を腕に配ってから押し込めばよい)。`accumulate` では IO の関数の worker (`$io`、トークンを引数にとる) の
  呼び出しになったので影響しなかったと見られる。
