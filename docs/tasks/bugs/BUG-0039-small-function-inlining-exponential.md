# BUG-0039: 小さい関数の連鎖をインライン化すると 1 回の簡約で指数的に複製し、Core の最適化が終わらない

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0097 (文字列リテラルの長さの畳み込みを入れた後の `tests/cirkalang/run/v002/strings.cirka`)
- 関連: BUG-0021, FEAT-0094, FEAT-0097

## 現象

FEAT-0097 で文字列リテラルの `bytes_len` を畳み込むと、`tests/cirkalang/run/v002/strings.cirka` のコンパイルが Core の最適化で終わらなくなった。
`cargo test --test cirkalang` ではこのケースのスレッドが 12 時間、メモリを 24 GB (実メモリ 10 GB + スワップ 14 GB) まで使い続け、
ホストのメモリを使い切った。`cirka build --time-passes` では「lower to Core」の後の「Core optimizations」が終わらない
(120 秒で 6.2 GB)。文ごとに分けると `S.trim` を含む 2 文だけが終わらない:

```
putStrLn (show (S.trim (S.take 16 t)))          -- t = toStr "  hello, world  \nsecond line\r\nthird\n"
putStrLn (show (S.splitOn ", " (S.trim t)))
```

## 再現手順

FEAT-0097 の畳み込みを入れたうえで:

```sh
cirka build --time-passes tests/cirkalang/run/v002/strings.cirka   # Core optimizations が終わらない
```

Core だけでは、次の関数を 2 か所から呼ぶ小さい関数の連鎖 (`k0 … k39`、各 25 ノード程度) を最適化すると再現する
(`crates/cirka-core/src/tests.rs` の `chain_of_small_calls`)。

## 期待される動作

インライン化で増えるコードは有界で、最適化はすぐ終わる。

## 原因

- リテラルの長さが畳み込まれると、`trimStart` の `skipSpace s i j` は `i` と `j` がリテラルのループになり、FEAT-0094 の展開が
  `i` ごとの複製 `skipSpace$w$k0_16`, `…$k1_16`, … を作る。文字列 `s` は引数のままなので各複製のバイトの検査は決まらず、
  空白のときの 2 つの分岐 (`b == 32` と `9 <= b <= 13`) がどちらも次の複製を末尾呼び出しする。2 か所から呼ばれる複製は関数のまま残る
  (展開の設計どおり)。
- 簡約器は小さい関数 (`INLINE_SIZE` = 60 ノード以下) をすべての呼び出し位置でインライン化し、インライン化した本体の中の候補も
  その場でインライン化する。候補の本体は 1 回の簡約の始めの大きさのままなので、`k0` を入れると `k1` が 2 つ、`k2` が 4 つ…と、
  1 回の簡約の中で 2^n 倍に複製される (次の回では大きくなった本体がもう小さくないので止まるが、その前に終わらない)。
- BUG-0021 の複製の数え方 (`COPY_BUDGET`) は、1 か所からしか呼ばれない関数にしか適用していなかった。

## 修正

- [x] 回帰テストを追加: `opt_bounds_the_copies_of_small_functions` (40 個の連鎖。修正後は 561 ノードから 24,703 ノード、0.5 秒以内)
- [x] 小さい関数も、インライン化される呼び出し元の複製で増える呼び出し (大きさ × (複製の数 − 自身の呼び出しの数)) が
  `SMALL_COPY_BUDGET` = 4,000 ノード以下のときだけインライン化する。wrapper と別名 (ループを切る位置でないもの) は従来どおり数えずにインライン化する
  (`crates/cirka-core/src/opt.rs` の `Inline::Small`、06-ir.md §2)

## メモ

- 影響の確認 (修正の前後のコンパイラで `--emit=llvm --dump=core-opt,mir-opt -O2`): bench/ の 14 プログラムは最適化後の Core と
  MIR が完全に同じ。CirkaLang の実行テスト 95 個のうち変わったのは 2 つだけ: `os_tool` (Core の変数番号だけ)、`vulkan_compute`
  (Core が 1,052,348 → 1,047,420 バイト、MIR は局所的な並び替え)。
- FEAT-0097 の畳み込みと合わせると、`v002/strings` の Core の最適化は 1.37 秒 (デバッグビルドのコンパイラ)、35 MB で終わる。
- テストの実行が 1 ケースの暴走でホストのメモリを使い切った件 (テストハーネスにケースごとの時間・メモリの上限がない) は別のバグとして扱う。
