# FEAT-0252: 数学・線形代数ラッパー

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P2
- 依存: FEAT-0250, FEAT-0256
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

specs 02 の方針で libm と OpenBLAS/LAPACKE の安全ラッパー。次元を型で表す。

## 作業内容

- [x] Dockerfile に OpenBLAS/LAPACKE を追加 (`libopenblas-dev`・`liblapacke-dev`)。`scripts/ci.sh` に「BLAS and LAPACK」の段
- [x] std.math (`Std.Math`: libm の関数を `extern "C" link "m"` で宣言した純粋な関数と定数・判定、spec-v0.0.6.md §4.1)
- [x] cirka-linalg (ライブラリの根 `lib/cirkalang/linalg` の `Linalg`: `Matrix m n`、純粋版 `gemm` (`c` が共有されていなければ in-place) と
      in-place 版 `gemmIn` (`Unique`)、`mul`・`solve`・`inverse`・`det`・転置・要素ごとの演算。spec-v0.0.6.md §4.2)
- [x] `Foreign` に unboxed 配列と C のメモリの間のコピー (`pokeUArray`・`peekUArray`・`peekUArrayInto`) と、ライブラリ用の `primUnsafeRunIO` (§4.3)
- [x] テストの `-- requires:` に `lib:<名前>` (`lib<名前>.so` を読み込めなければ飛ばす)
- [x] 設計: spec-v0.0.6.md §4、09-libraries.md §1A・§1.5、ADR-098

## 受け入れ条件

- [x] 行列積・連立一次方程式の例が参照実装と一致 (`tests/cirkalang/run/v006/linalg.cirka`: CirkaLang で書いた積と比べ、
      解・行列式・期待出力は Python の有理数の計算と一致することを確かめた。両方の並び、`-O0`/`-O2`、インタプリタとネイティブ)
- [x] 次元不一致が型エラー (`tests/cirkalang/fail/linalg/dims`: E1200。`at` の添字が範囲の外なら `index`: E0501)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

### 結果 (2026-11-16)

- テスト: `run/v006/std_math` (libm の関数。正確な値はそのまま、ほかは 9 桁に丸めて表示)、`run/v006/uarray_ptr` (配列と C のメモリのコピー。
  共有されていない配列への `peekUArrayInto` はコピーしないことを `max-array-copies: 1` で確かめる)、`run/v006/linalg` (`requires: lib:openblas lib:lapacke`)、
  `fail/linalg/{dims,index}`。
- `Std.Math` のテストで、ネイティブのプログラムが `cbrt` などをランタイムの中の compiler-builtins から取っていたことが分かった (BUG-0028、修正済み)。
- `gemm` の in-place: 関数 (`accumulate`) の中で繰り返す `gemmIn` はコピーしない。`Main.main` の中の `gemm` の 1 つは、動作がクロージャとして確保されて
  `c` の配列を持つためにコピーされる (結果は同じ)。最適化の課題として FEAT-0088 に起票し、`linalg.cirka` は `max-array-copies: 2` とした。
- 設計の判断 (ADR-098): 行列は `UArray Float` と並びで表し、BLAS / LAPACK の呼び出しのたびに要素を C のメモリにコピーする。
  要素の型の引数 (`Matrix m n a`) は、BLAS のほかの型が CirkaLang にないので持たない。
- 計測 (Docker、`OPENBLAS_NUM_THREADS=1`、hyperfine 10 回。512×512 の積を 20 回、`docs/artifacts/bench-0252/`):

  | プログラム | 時間 | うちシステム時間 |
  |------------|------|------------------|
  | `Linalg.mul` (CirkaLang、-O2) | 347.6 ± 4.5 ms | 60.6 ms |
  | `cblas_dgemm` を C から直接 | 255.3 ± 9.1 ms | 5.6 ms |

  差 (1 回あたり約 4.6 ms) の多くは、呼び出しごとに確保する 2 MB の C のバッファ 3 つと結果の配列のページフォールトで、memcpy 自体 (6 MB) は 1 ms に満たない。
  配列の領域を直接 BLAS に渡してバッファをなくすことを FEAT-0257 に起票した。

### 依存型・部分集合型の評価

- 次元を型に持つと、積・和の次元の誤りが型検査で見つかり (E1200)、`at` の添字は定数ならそのまま、計算した添字は `if` などの条件から示せる。
  消去されるので実行時のコストはない。行列を作る関数は次元の値をとって型に使う依存型の関数で、実行時に決まる大きさの行列も同じ型で扱える。
- 一方、行列の次元を値として取り出す `rows` / `cols` の結果と型の次元の関係 (`rows a == m`) は型に現れないので、次元が型変数の関数の中では
  `at` の添字を示せず、`get` (`Maybe`) を使うことになる。結果の型を `{r : Int | r == m}` にするには、構成子のフィールドに型の引数を参照する
  部分集合型を書けることと、`eye (rows a) : Matrix n n` のように値の等しさから型の等しさを導くこと (または型の等しさの証明を値で持ち回ること) が要る。
  第二段階で、型の引数を実行時にも使える (多重度 ω の暗黙の引数) ようにすると自然に書ける。

