# FEAT-0068: 配列の作成のコスト (遅い段階の worker/wrapper・`newArray` の初期化の省略・`calloc`)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0056, FEAT-0123
- 設計: [06-ir.md](../../design/06-ir.md) §1.4, [05-runtime.md](../../design/05-runtime.md) §3, [16-performance.md](../../design/16-performance.md) §4

## 概要

array-sort の Rust との差の内訳 ([bench-report](../../design/bench-report.md)、FEAT-0067) で、配列の作成と乱数列の書き込みが
0.049 s と 0.025 s で差が 0.024 s あった (400 万要素)。原因は次の 2 つ。

1. **乱数の種の箱**: `build` のループの種 `s` は、抜ける経路で使わないので正格でなく、`I#` の箱のまま渡される。
   要素ごとに `cirka_drop_reuse` を呼ぶ (400 万回、128 M 命令、すべての間接分岐)。FEAT-0056 の規則は「すべての呼び出しが評価済みの値を渡す」
   引数を unboxing するが、`s'` が値になるのは `strictify` と `float_in` の後で、最後の worker/wrapper はその前に走る。
   また `$w` で終わる関数は worker/wrapper の対象外なので、後から機会が来ても分割できない。
2. **配列全体への書き込みが 3 回**: `cirka_bytes_new` のゼロ埋め、`newArray` の `fill0` のゼロの書き込み、`build` の書き込み。
   Rust の `vec![0; n]` は `calloc` で、OS が返すゼロのページをそのまま使うので 1 回で済む。

対応:

- `float_in` の後に worker/wrapper を走らせ、`$w` の関数も分割の対象にする。分割するたびに箱の引数が減るので止まる。
- `newArray` は、要素の表現がすべて 0 のビットならゼロ埋めをそのまま使う (`UElem` にメソッドを足す)。
  `n <= 0` では要素を評価しない (今の遅延の意味を変えない)。`-0.0` はビットが 0 でないので省略しない。
- `cirka_bytes_new` の大きなオブジェクトは `calloc` で確保する (新しいページならゼロ埋めの走査がいらない)。

## 作業内容

- [x] `float_in` の後の worker/wrapper と `$w` の分割
- [x] `newArray` の初期化の省略
- [x] `cirka_bytes_new` の `calloc`
- [x] ベンチで前後を計測する (命令数と、負荷の低いときの時間)

## 受け入れ条件

- [x] 評価済みの値を渡される worker の引数が unboxing される Core のテスト
      (`worker_wrapper_splits_workers_again`、`opt_unboxes_parameters_that_become_values_late`)
- [x] 確保済みのメモリを再利用してもゼロになるランタイムのテスト (小さい・大きいオブジェクト、`zeroed_memory_is_zero_after_reuse`)
- [x] `newArray` の意味が変わらない実行テスト (`-0.0`、`n <= 0` で要素を評価しないこと、各要素型、`uarray_new`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

### 計測 (2026-11-06)

array-sort `own.chs`。ホストの負荷が低いときに交互に 21 回実行した `wait4` の最良値 (ユーザーとシステムの CPU 時間の和)。

| 版 | 前 | 後 | Rust |
|----|---:|---:|---:|
| 1,000,000 要素 | 0.112 s | **0.103 s** | 0.087 s |
| 4,000,000 要素 | 0.454 s | **0.433 s** | 0.360 s |

段階ごと (400 万要素。段階の途中で止める版を作り、関数の整列を 64 バイトに固定):

| 段階までの時間 | 前 | 後 | Rust |
|----------------|---:|---:|---:|
| 配列の確保 | 0.023 s | **0.003 s** | 0.002 s |
| 乱数列の書き込みまで | 0.049 s | **0.023 s** | 0.021 s |
| ソートまで | 0.439 s | 0.411 s | 0.344 s |
| 全体 | 0.459 s | 0.427 s | 0.366 s |

配列の作成は Rust とほぼ同じになった。ソートの時間 (ソートまでの時間から乱数列の書き込みまでの時間を引いたもの) は変わらない (0.390 s と 0.388 s)。

callgrind (400 万要素、負荷にもコードの配置にも左右されない):

| | 前 | 後 | Rust |
|---|---:|---:|---:|
| 命令数 | 2,154 M | **1,919 M** (-10.9 %) | 1,308 M |
| 最終キャッシュの書き込みミス | 1.50 M | **0.50 M** | 0.50 M |
| 間接分岐 | 4,002,698 | **2,704** | 933 |

### 他のプログラムへの影響

- 確保回数: `bench/` のすべてのソース (aCHC の `own.chs`・`plain.hs`、CirkaLang の `main.cirka` の計 14 本) で、同じか減った。出力は同一。
  箱の作り直しで確保が増えたものはない。
- `$w` の再分割が起きたのは array-sort `own.chs` (2 関数) と mandelbrot `plain.hs` (4 関数) だけ。
- 命令数 (callgrind): mandelbrot `plain.hs` が -0.15 %、ほかの 8 本は ±0.01 % 以内で変わらない。
- 時間: 他のベンチの計測は途中でホストの負荷が上がった (n-body の値が 2 倍近くになった) ので使わない。命令数が変わらないので時間も変わらないと判断した。

### テストの確かさ

規則を 1 つずつわざと壊し、テストが失敗することを確かめた。

| 壊した規則 | 失敗するテスト |
|------------|----------------|
| `n <= 0` でも要素を評価する | `uarray_new` (`undefined` で panic) |
| `Double` を値で比べる (`-0.0` がゼロ扱い) | `uarray_new` |
| `Int` / `Char` / `Bool` のすべての値をゼロ扱い | `uarray_new` |
| 大きなオブジェクトをゼロにしない | `zeroed_memory_is_zero_after_reuse` |
| 小さなオブジェクトをゼロにしない | `zeroed_memory_is_zero_after_reuse` |

`worker_wrapper_splits_workers_again` と `opt_unboxes_parameters_that_become_values_late` は変更の前に失敗することを確かめた。
既存の `opt_worker_wrapper_unboxes_parameters_passed_evaluated` (分割の回数) と
`worker_wrapper_keeps_lazy_parameters_passed_unevaluated` (意味を変える unboxing をしないこと) もそのまま通る。
