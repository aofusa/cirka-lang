# BUG-0020: 遅延位置の構築子の正格フィールドに入れた捕捉変数・仮引数が要求前に評価される

- 状態: todo
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

- [ ] 回帰テストを追加 (`tests/achc/diff/`)
- [ ] 仮引数・捕捉変数が評価済みであることを示せるとき (呼び出しの事実、FEAT-0066 の `whnf_params`、正格な言語の CirkaLang) だけ安全とする
- [ ] n-body `main.cirka` などで性能の変化を計測する (BUG-0014 の時点では一律にサンクにすると +28% 命令)

## メモ
