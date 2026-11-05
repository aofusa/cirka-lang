# BUG-0007: 導出した Read が演算子のフィールド名と逆引用符の中置構築子を読めない

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0119 (互換計測、GHC testsuite の deriving/drvrun004, drvrun018)
- 関連: FEAT-0110, FEAT-0119

## 現象

`deriving Read` したインスタンスで次の入力の `read` が `Prelude.read: no parse` になる (GHC は読める)。

- 演算子名のレコードフィールド: `data Hash = Hash { (#) :: Int }` に対する `"Hash {(#) = 3}"`
- 逆引用符で中置にした構築子: `data Foo = Int `MkFoo` Int` に対する ``"3 `MkFoo` 5"``

## 再現手順

```sh
scripts/compat.sh   # CIRKA_COMPAT_FILTER=deriving/drvrun0
```

## 期待される動作

GHC と同じく読める (Haskell 2010 §11.4)。

## 原因

導出コード (`crates/achc-typeck/src/deriving.rs`) がフィールド名を識別子として `lex` の結果と比べ、
英字の構築子の中置形 (逆引用符) の分岐を生成していない。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ
