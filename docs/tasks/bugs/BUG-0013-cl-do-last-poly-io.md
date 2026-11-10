# BUG-0013: CirkaLang の `do` の最後の文に `exit` などの多相な IO を書くと型エラーになる

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-0216 (tests/cirkalang/test/digits.cirka)
- 関連: FEAT-0203

## 現象

`do` ブロックの最後の文が結果の型に多相な IO (`exit : Int -> IO a`) だと、期待される型が `IO ()` でも型エラーになる。

```
error[E1200]: type mismatch
  |   exit 3
  = note: expected `IO ?111`
  = note:    found `IO (?112 _)`
```

## 再現手順

```
main : IO ()
main = do
  putStrLn "x"
  exit 3
```

を `cirka run` する。`main = exit 3` や `(exit 3 : IO ())` と注釈した場合は通る。

## 期待される動作

型検査を通り、`x` を出力して終了コード 3 で終わる。

## 原因

`do` の脱糖は最後の文を `\_ => rest` の本体として検査する。`>>=` の結果型のメタ変数 `?111` は束縛子の外で作られ、`exit` の型引数 `?112` は束縛子の中で作られて文脈の変数に依存する (`?112 _`)。単一化の two-meta (異なるメタ同士) の場合が meta-solve にそのまま流れ、`?111 := ?112 x` を試して、スコープ外の `x` を含むので失敗していた。

## 修正

- [x] 回帰テストを追加 (`tests/cirkalang/run/v002/do_last_poly_io`)
- [x] 修正: `unify_shallow` に異なるメタ同士の場合を追加し、より多くの変数が見えるほう (スパインが長いほう) から解く。失敗したらもう一方を解く

## メモ

- FEAT-0216 のテストの回避用注釈 `(exit 3 : IO ())` は修正で不要になったので外した。
