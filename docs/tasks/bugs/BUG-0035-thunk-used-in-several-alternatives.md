# BUG-0035: 複数の選択肢で使う遅延束縛が、使わない反復でもサンクとして確保される

- 状態: done
- 重大度: major
- 発見箇所: BUG-0034 の調査
- 関連: BUG-0034, FEAT-0112 (float-in)

## 現象

aCHC で `when (i `mod` 25000000 == 0) (print i)` を含むループ (1 億回) が 2.5 s かかる。結果は正しい。
`-O2` の Core では、`print i` の動作 `m` が遅延 `let` (サンク) としてループの各反復で確保される。
`mod` の展開で `case` が符号ごとの 3 つの選択肢に分かれ、`m` はその 3 か所で使われる。

## 再現手順

```haskell
import Control.Monad (when)

loop :: Int -> Int -> IO Int
loop 0 acc = pure acc
loop i acc = do
  when (i `mod` 25000000 == 0) (print i)
  loop (i - 1) (acc + i)

main :: IO ()
main = loop 100000000 0 >>= print
```

```sh
achc build -O2 --dump=core-opt -o w when.hs   # Main.loop$io$w の先頭に (let ((m (-> Unit# Unit) ...
./w                                           # ホストで 2.5 s
```

## 期待される動作

`m` は使う選択肢の中でだけ作る (出力する反復でだけ)。

## 原因

float-in (`strict::float_in`) の `sink` が、次の 2 つの所で束縛を外に残していた。

- 後ろの `let!` の値が束縛を使うと、そこで止まる (値の中の `case` の選択肢へは移さない)。`do` の途中の `when c act` では、
  動作 `m` を使うのは `c` の `case` (次の `let!` の値) だけである。
- 束縛を使う選択肢が複数のときは、ある選択肢が必ず評価し、かつ右辺が 40 ノード以下の場合だけ写していた (FEAT-0064)。
  `when` の動作には使わない経路 (出力しない反復) があるが、必ず評価する選択肢はない (入れ子の `case` の片方だけが使う)。
  動作 (`show` と `putStr` の部分適用) は 50 ノードある。

## 修正

- `sink` は、後ろの `let!` の値だけが使う束縛を、値の中の `case` の選択肢に届くなら値の中へ移す
  (届かないなら、simplify が外へ出し直すので移さない)。
- 使う選択肢が複数でも、束縛を使わない経路があれば (使わない選択肢がある、または選択肢の中の `case` の一部の経路だけが使う)、
  写しで増えるコード (右辺の大きさ × (使う箇所 − 1)) が 240 ノード以下なら選択肢ごとに写す (`worth_copying`)。
  実行される写しは 1 つなので、仕事は増えずコードだけが増える。必ず評価する選択肢がある場合の写し (40 ノード以下) は従来どおり。

- [x] 回帰テストを追加 (`cirka-core` の `opt_moves_an_action_into_the_alternatives_that_run_it`、
  `tests/achc/run/h3/when_loop.hs`: 確保 200 以下。修正前は 100 114)
- [x] 修正

## メモ

- 再現手順のループ (ホスト、1 億回): 2.5 s → 0.16 s。`when_loop.hs` (10 万回) の確保は 100 114 → 114。
  同じ形の CirkaLang 版 (`when.cirka`、BUG-0034 で直った) は変わらない。
- 全テストの期待出力は変わらなかった。
- ベンチの `-O2` の MIR: CirkaLang の binary-search・binary-trees・cli-grep で、選択肢の一方でしか使わない確保 (クロージャ・箱・文字列のビュー) が
  その選択肢の中に移った。aCHC の版では、`Integer` の `+` と `rem` の小さい整数の経路がサンクを作らなくなり (大きい整数の計算が選択肢ごとに写された)、
  `read` の字句解析と `main` の一部が変わった。
- 命令数 (callgrind、Docker、縮めた大きさ。修正前 → 修正後): cli-grep (`main.cirka`、10 万行) 234.0 M → 208.1 M (0.889)、
  fannkuch-redux (`plain.hs`、n = 8) 3188.7 M → 3146.0 M (0.987)。array-sort・binary-search・binary-trees・mandelbrot・n-body・
  spectral-norm の各版は ±0.01 % 以内。遅くなった題材はない。
