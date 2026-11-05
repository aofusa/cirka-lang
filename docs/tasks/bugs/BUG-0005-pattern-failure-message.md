# BUG-0005: パターン照合の失敗の実行時エラーが GHC の文言・位置と異なる

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0115
- 関連: FEAT-0104, FEAT-0117

## 現象

関数・`case`・ラムダのパターン照合が失敗すると、aCHC は `prog: non-exhaustive patterns` と出す。
GHC は `prog: f.hs:(3,1)-(4,10): Non-exhaustive patterns in function f` (`case` なら `... in case`、
ラムダなら `... in lambda`) のように位置と種類を出す。差分テスト (`.stderr`) で一致しない。

## 再現手順

```haskell
f :: Int -> Int
f 0 = 1

main :: IO ()
main = print (f 2)
```

```sh
achc run f.hs   # f: non-exhaustive patterns
```

## 期待される動作

GHC と同じ `f.hs:2:1-7: Non-exhaustive patterns in function f`。

## 原因

脱糖器 (`achc-desugar` の照合のコンパイル) が失敗時に固定の文言の `Panic` を作っている。関数全体の範囲と照合の種類
(関数・case・ラムダ) を型検査器から渡しておらず、ソースマップも持っていない。レコードの実行時エラー (FEAT-0115) は
型検査器の `ghc_loc` で位置を付けているので、同じものを照合の節にも付ければよい。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ
