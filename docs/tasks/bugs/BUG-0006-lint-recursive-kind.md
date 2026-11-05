# BUG-0006: 関数型を介して自身を含むデータ型で Core Lint がスタックオーバーフローする

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0119 (互換計測、GHC testsuite の codeGen/cgrun050)
- 関連: FEAT-0006, FEAT-0119

## 現象

`newtype F = F (Int -> Val)` と `data Val = VFn !F | VInt Int` のように、関数型 (や newtype) を介して自分自身を
フィールドに含むデータ型があると、脱糖後の Core Lint がスタックオーバーフローでコンパイラごと異常終了する
(`fatal runtime error: stack overflow`)。

## 再現手順

```sh
achc run tests/achc/run/h2/recursive_newtype.hs
```

## 期待される動作

型検査を通るプログラムとして実行できる (GHC と同じ出力)。

## 原因

データ型の種 (Type / LType、03 §1.1) の計算 `data_kind` は訪問中の型の一覧で再帰を止めていたが、フィールドが
型構築子の適用でない場合 (関数型など) の `field_linear` が `kind` を呼び、`kind` は空の一覧で `data_kind` を始め直していた。

## 修正

- [x] 回帰テストを追加 (`tests/achc/run/h2/recursive_newtype.hs`)
- [x] 訪問中の型の一覧を Linter が持ち、すべての経路で共有する
