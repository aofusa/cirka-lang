# BUG-0020: 遅延位置の構築子の正格フィールドに入れた捕捉変数・仮引数が要求前に評価される

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0083 の調査 (BUG-0014 の「残した穴」の具体例)
- 関連: BUG-0014, FEAT-0083

## 現象

```haskell
data P = P !Int

g :: Int -> Int
g k
  | k > 10 = g (k - 1)
  | k == 0 = error "boom"
  | otherwise = k + 41

f :: P -> Int -> Int
f _ 0 = 0
f p n = case p of P x -> x + f p (n - 1)

main = do
  args <- getArgs
  let d = length args
      t = g d
  print (f (P t) d)
  print (f (P t) (d * 2))
```

引数なしで実行すると GHC は `0` を 2 行出力するが、aCHC は `-O0` で `boom` で終了する (`-O2` では `0` を出力する)。
`f` は `n == 0` のとき `p` を要求しないので、`P t` は作られず、`t` も評価されないはずである。

## 再現手順

```sh
target/release/achc build -O0 -o /tmp/p conarg.hs && /tmp/p
```

## 期待される動作

遅延位置の構築子は要求されたときに作る。正格フィールドの評価もそのときに行う (Haskell 2010 §4.2.1、BUG-0014)。

## 原因

MIR への下ろしの `Lowerer::safe_now` は、同じ関数の中で作ったサンクだけを危険とし、仮引数と捕捉変数は評価済みでなくても安全とみなす
(BUG-0014 で意図して残した穴)。`-O0` では `do` の後続がクロージャ・サンクになり、`let` で作ったサンク `t` が捕捉変数として渡るため、
`f (P t) d` の引数 `P t` がその場で作られて `t` が `force` される。

## 修正

- [x] 回帰テストを追加 (`tests/achc/diff/strict_field_captured_thunk.hs`、MIR の単体テスト `lazy_constructors_wait_for_unevaluated_variables`)
- [x] 仮引数・捕捉変数が評価済みであることを示せるとき (呼び出しの事実、FEAT-0066 の `whnf_params`、正格な言語の CirkaLang) だけ安全とする
- [x] n-body `main.cirka` などで性能の変化を計測する (BUG-0014 の時点では一律にサンクにすると +28% 命令)

## メモ

**修正。** `Lowerer::safe_now` は、評価済みと分からない変数 (`whnf: false` のポインタ) をすべて危険とする。
以前は同じ関数で作ったサンクだけを危険とし、仮引数と捕捉変数は安全とみなしていた。あわせて、捕捉変数の「評価済み」を捕捉した側から引き継ぐ
(`lifted_env` が捕捉変数ごとに記録し、サンク・クロージャ・持ち上げた局所関数の本体で使う)。捕捉した値は同じ値なので評価済みのまま。
使われなくなった `Fb::thunks` (この関数で作ったサンクの集合) は削除した。

**CirkaLang の扱い。** 正格な言語なので早く評価しても意味は変わらないが、言語ごとの例外は設けなかった。
計測で費用が出なかったため (下記)。MIR への下ろしは言語に依存しないまま。

**計測 (callgrind の命令数、HEAD `c5761dc` との比較、2026-11-14)。** 11 の題材すべてで ±0.01% 以内
(spectral-norm `plain.hs` 64,630,188 → 64,630,120、n-body `main.cirka` 57,732,651 → 57,732,789、fannkuch-redux `plain.hs` は 0.07% 減)。
MIR のサンクの数は spectral-norm `main.cirka` の初期化で 1 つ増えただけで (作ってすぐ捨てる組)、`force` は捕捉変数の引き継ぎで各題材 0〜17 個減った。
BUG-0014 の時点の +28% は、その後の評価済みの引数の解析 (FEAT-0066)・unboxing (FEAT-0078)・先行評価 (FEAT-0083) で正格な位置が増え、起きなくなっていた。

**ミューテーション。** `safe_now` で評価済みでない変数を安全に戻すと単体テストの 1 つ目の表明が失敗する (修正前のコンパイラでは回帰テストが `-O0` で `boom` になる)。
捕捉変数の引き継ぎをやめると単体テストの 3 つ目の表明が失敗する。
