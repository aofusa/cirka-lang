# FEAT-0078: 評価済みの引数の判定で unboxed の値を値として数える

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0056, FEAT-0068
- 設計: [06-ir.md](../../design/06-ir.md) §1.4

## 概要

mandelbrot の `plain.hs` は内側のループの引数が箱のままで、反復ごとにサンクと箱を作っていた (Rust の 25 倍かかっていた)。

`go k zr zi` は `k >= 50` の枝で `zr` を使わないので正格ではなく、FEAT-0056 の条件「すべての呼び出しが評価済みの値か、先に評価しても安全な式を渡す」を満たしたときだけ unboxing される。
判定の `ww::evaluated_params` は `Double#` のような unboxed 型の束縛を評価済みと数えていなかったので、`case d of D# x -> D# (x * x)` のように unboxed の値しか読まない引数も「安全」と判定できなかった。

unboxed 型の値はサンクになり得ないので、仮引数でも `case` で取り出したフィールドでも評価済みとして数える。

## 作業内容

- [x] `ww.rs`: `params_at_calls` の仮引数、`CallWalk::expr` の `case` のフィールド、`speculatable` の `case` のフィールドで unboxed 型を値として数える
- [x] 単体テスト (`evaluated_params_count_unboxed_binders`、`unboxed_parameters_make_later_arguments_values`)
- [x] 実行テスト `tests/achc/run/h3/speculated_loop.hs` (`max-allocs`・`max-forced` で確保の回数を抑える。出力は GHC と一致)
- [x] 全ベンチで前後を計測する

## 受け入れ条件

- [x] 上記のテストがある
- [x] 全ベンチの出力が変わらず、命令数が増えない (計測をメモに記録)
- [x] `scripts/ci.sh` が通る

## メモ

mandelbrot `plain.hs` の計測 (FEAT-0076 → この変更):

| 計測 | FEAT-0076 | FEAT-0078 | GHC -O2 | Rust |
|---|---:|---:|---:|---:|
| 命令数 (n=100、callgrind Ir) | 328.6 M | **4.0 M** | | |
| 時間 (n=1000、CPU 時間の最良値) | 2.451 s | **0.099 s** | 0.430 s | 0.098 s |

`speculated_loop.hs` の確保は 1,008,504 回 → 22 回、評価したサンクは 490,286 個 → 8 個。
ほかの 13 個のベンチは LLVM IR が同一で、命令数の差は数百命令以内 (`array-sort plain.hs` はこの変更の前からビルドできない)。

ミューテーション: 次の 5 種類を試し、すべて単体テストが検出した。仮引数の判定を外す / `case` のフィールドの判定を外す (呼び出しの位置、式の中の 2 か所) / boxed のフィールドまで値として数える (同じく 2 か所)。

検討して採らなかったこと:

- **worker/wrapper の遅いラウンドの繰り返し。** 最初は `speculatable` の `case` のフィールドを見落としていて、分割を繰り返すと解消したので繰り返しのループを入れた。
  `speculatable` を直すと 1 ラウンドで済み、繰り返しの有無で全ベンチの LLVM IR が同一になったので外した。
- **正格フィールドも値として数えること。** 命令数は spectral-norm `main.cirka` で 20.0 M → 13.7 M、n-body `main.cirka` で 592.5 M → 571.3 M と減るが、
  binary-trees は `own.chs` で 45.6 M → 50.6 M、時間 (深さ 18) も 1.252 s → 1.547 s と悪化し、spectral-norm の時間は変わらなかった (0.179 s → 0.177 s) ので入れない。
  `check$w` の `force` が消えて借用の推論が引数を `@borrowed` にし、呼び出し側の `drop` が木をもう一度たどるようになるのが原因。借用の推論が走査の後に解放できる引数を所有のままにできれば、改めて検討できる。
