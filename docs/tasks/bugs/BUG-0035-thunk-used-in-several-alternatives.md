# BUG-0035: 複数の選択肢で使う遅延束縛が、使わない反復でもサンクとして確保される

- 状態: todo
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

float-in (`strict::float_in`) は、遅延束縛を使う選択肢が 1 つのときだけその中へ移す。使う選択肢が複数だと外に残る。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正 (案: 小さい右辺は、使う選択肢それぞれに複製して移す。実行されるのは 1 つなので仕事は増えず、コードだけが増える)

## メモ
