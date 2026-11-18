# FEAT-0088: 場合分けの後の IO の動作をクロージャにしない

- 状態: done
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

- [x] 再現する最小の Core を作り、どの変形 (場合分け・結合点への引数の押し込み、`withOrder` の 2 つの腕の結合点) が効いていないかを調べる
- [x] IO の関数を場合分けの後で呼ぶループのベンチを作り、確保数と時間を記録する (16-performance.md §4)
- [x] 修正し、`linalg.cirka` の `max-array-copies` を 1 にする

## 受け入れ条件

- [x] `linalg.cirka` の `gemm` がどれも in-place になる (`max-array-copies: 1`)
- [x] ベンチの確保数が減り、ほかのベンチが遅くならない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 結果は正しく、コピーが 1 回増えるだけである。
- 見込み (未確認): `opt.rs` の `apply` は、`(case s of p -> e) a` を関数の型の `case` のときだけ腕に押し込み、`cast` を通す適用は
  `cast` の中がラムダか `let` のときだけ扱う。IO の動作は `Unit# -> a` を `IO a` に `cast` した値なので、`(cast (case …) (Unit# -> a)) token` が
  押し込まれずに残る可能性がある (`cast` を腕に配ってから押し込めばよい)。`accumulate` では IO の関数の worker (`$io`、トークンを引数にとる) の
  呼び出しになったので影響しなかったと見られる。
- FEAT-0257 で `Linalg` の `gemm` を書き換えた後は、`linalg.cirka` にこのコピーは現れない。再現には FEAT-0252 の版の `Linalg`
  (コミット 424dd3d、`peekUArrayInto` で結果を読む `gemmIO`) と `linalg.cirka` を使う。
- BUG-0034 で、`let! t = (case … の腕ごとの λ) in let! x = (cast t τ) w in …` の形 (`do` の途中の `if` が選んだ動作) を直した
  (`t` をその呼び出しに代入し、キャストと適用を腕に配る)。この再現が同じ形なら直っているはずなので、まずそれを確かめる。

## 結果

BUG-0034 の修正で直っていた。新しいコードの変更はなく、回帰テストと記録を加えた。

- 原因: FEAT-0252 の `Linalg` を使う `linalg.cirka` の `-O2` の Core では、`Main.main` の 2 つ目の `gemm` が次の形で残っていた。

  ```
  let! t = case Main.a34 of MkMatrix … -> case Main.b42 of MkMatrix … -> case c of MkMatrix … ->
             let! size = (case (prim le n 8) of 0 -> n; _ -> 8) in      -- allocaArray (max …)
             cast (λw. … cblas_dgemm … bytes_unique xc …) (IO Matrix)
  in let! x = cast ((cast t (-> Unit# Matrix)) unit) Matrix              -- primUnsafeRunIO
  in case x of …
  ```

  単一選択肢の `case` を `let!` の外へ移す規則 (FEAT-0066) は調べる値が局所変数のときだけ働き、トップレベルの値 (`Main.a34` など) では働かない。
  そのため動作 (λ) が `let!` の右辺に残り、行列のフィールドを持つクロージャになって、`bytes_unique` の時点で `c` の配列を共有していた。
  これは BUG-0034 の形 (次に呼ぶだけの `let!`) で、`t` を呼び出しに代入してキャストと適用を選択肢に配る規則が効く。
  `accumulate` のような関数の中では行列が局所変数なので、BUG-0034 の前からクロージャはなかった。
- `linalg.cirka` (FEAT-0252 版の `Linalg` と試験、コミット 424dd3d から取り出し、今のコンパイラ・Docker): BUG-0034 の規則を外すとクロージャ 2・
  配列のコピー 2・確保 1799、規則ありでクロージャ 0・コピー 1 (`c32`。後でも使うので共有される)・確保 1793。出力は同じ。
  今の `Linalg` (FEAT-0257) の `gemm` は配列を直接 `modifyUArrayWith` に渡すので、今の `linalg.cirka` にはこの形が現れない (コピー 9 回は共有された行列の分)。
- テスト: `cirka-core` の `opt_runs_an_action_chosen_by_cases_in_a_pure_function` (トップレベルの値の `case` と `max` の `let!` の後の動作を
  `unit` に適用する純粋な関数。BUG-0034 の規則を外すと失敗する)。利用者のプログラムは基本操作を宣言できない (E1009) ので、
  純粋な文脈で動作を走らせる形は `Core` で試す。ループの側は `tests/cirkalang/run/v007/io_after_case.cirka`
  (節で定義した IO の関数を `withOrder` の結合点の後で 1000 回呼ぶ。配列のコピー 0、確保 76。IO の中では状態を受け取る worker の呼び出しになるので、
  BUG-0034 の前からクロージャはない)。
- ほかのベンチ: BUG-0034 の規則の有無で `bench/*` の `-O2` の MIR を比べた (Docker)。aCHC の版はすべて同一。
  CirkaLang の版は、`effects/direct.cirka` (BUG-0034 で 9 倍速くなった) のほかは起動時に引数を読む所 (既定値を選ぶ `case`) だけが変わり、
  クロージャ 1 つと間接呼び出しがなくなった。命令数 (callgrind、縮めた大きさ) は mandelbrot・n-body・spectral-norm・stencil・string-split・array-sort の
  `main.cirka` でそれぞれ 210〜741 命令減り、遅くなった題材はない。

