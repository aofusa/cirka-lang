# BUG-0026: 型の中の多重定義された演算 (`Tagged (2 + 5) Bool`) でインスタンスの解決が ICE になる

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0250 (impl の値の変数を試したとき)
- 関連: FEAT-0250

## 現象

値を引数に取る型の引数に、多重定義された演算 (`+` など) を書いた型注釈があると、その型のインスタンスを探すときに ICE になる。

```
internal compiler error: the solution of a constraint escapes its scope: `impl$Width$0`
error[E1210]: cannot infer this implicit argument or `_`
```

リテラルだけ (`Tagged 7 Bool`) なら起きない。impl の変数の多重度 (FEAT-0250 で値の変数を実行時に渡すようにした) とは関係なく、変数を宣言しない impl でも起きる。

## 再現手順

```
data Tagged (n : Int) (a : Type) = MkTagged a

trait Width a where
  width : a -> Int

impl Width (Tagged n a) where
  width _ = 0

main : IO ()
main = print (width (MkTagged True : Tagged (2 + 5) Bool))
```

```sh
cirka run Main.cirka
```

## 期待される動作

`0` を出力する (`2 + 5` は `7` に評価されるか、解けていないメタ変数を含んだままインスタンスが選ばれる)。

## 原因

(未調査) インスタンスの頭部の変数 `n` に、`Num` の辞書などの解けていないメタ変数を含む値が束縛され、
その解を制約のメタ変数に代入する単一化 (`traits.rs` の `assign`) が失敗している。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ
