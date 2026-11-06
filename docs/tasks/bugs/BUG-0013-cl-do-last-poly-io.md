# BUG-0013: CirkaLang の `do` の最後の文に `exit` などの多相な IO を書くと型エラーになる

- 状態: todo
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

未調査。`do` の脱糖 (`>>`) で期待される型が最後の文まで伝わらず、文脈の変数に依存するメタ変数 (`?112 _`) と
依存しないメタ変数 (`?111`) の単一化に失敗しているとみられる。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ

- FEAT-0216 のテストでは `(exit 3 : IO ())` と注釈して回避した。
