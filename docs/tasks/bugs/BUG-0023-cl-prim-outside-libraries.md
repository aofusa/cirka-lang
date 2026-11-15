# BUG-0023: 利用者のプログラムに `prim` 宣言を書け、検査のない配列の読み出しでメモリ安全を破れる

- 状態: todo
- 重大度: critical
- 発見箇所: FEAT-0244 の調査 (信頼する仕様をライブラリに閉じる仕組みを調べているとき)
- 関連: FEAT-0206, FEAT-0234, FEAT-0244

## 現象

spec-v0.0.1.md §5.6 は `prim name : T` を「コアライブラリの中だけで書ける」と定めるが、実装は検査していない。利用者のプログラムが
コンパイラの内部のプリミティブ (証明済みの添字を前提に境界検査を省く `primArrayIndexProved` など) を宣言して呼べるので、
`extern` を使わずにメモリ安全を破れる。

```
import Data.Array as A

prim primArrayIndexProved : {a : Type} -> A.Array a -> Int -> a

main : IO ()
main = do
  let xs = A.fromList [1, 2, 3]
  printInt (primArrayIndexProved xs 100000)
```

`cirka run` で `segmentation fault` (終了コード 139)。

## 再現手順

上のプログラムを `cirka run` する。

## 期待される動作

コンパイラに付属するライブラリ (`lib/cirkalang` の `core` と `std`) の外の `prim` 宣言は誤りになる。

## 原因

`prim` 宣言の検査 (`Elab::prim`) は名前がプリミティブの表にあるかだけを見ており、宣言したモジュールがどこから読まれたかを見ていない。

## 修正

- [ ] 回帰テストを追加
- [ ] ライブラリの外のモジュールの `prim` 宣言を誤りにする

## メモ
