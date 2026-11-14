# BUG-0019: 単一化が作るメタ変数で、メタ変数の位置の表がずれて ICE になる

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0235 (検証済みサンプルの作成)
- 関連: FEAT-0203

## 現象

部分集合型の述語に未定義の名前 (暗黙の型変数になる) を書くなどで、推論できないメタ変数が残ると
`internal compiler error: index out of bounds: the len is N but the index is N at crates/cirkalang-elab/src/elab.rs` になる。

## 再現手順

```sh
cirka check tests/cirkalang/fail/type/refine_unbound.cirka
```

## 期待される動作

E1210 (推論できない暗黙の引数) を報告する。

## 原因

メタ変数の位置 (`meta_spans`) と報告済みの印 (`meta_reported`) はメタ変数の番号で引くが、エラボレータの `fresh_meta` だけが表に追加していた。
単一化の刈り込み (pruning) もメタ変数を作るので、それ以降のメタ変数の番号と表の添字がずれていた (E1210 の位置も 1 つずれていた)。

## 修正

- [x] 回帰テストを追加 (`tests/cirkalang/fail/type/refine_unbound.cirka`)
- [x] 修正: メタ変数を作るときに表をその番号まで埋める (`new_meta`)。刈り込みで作ったメタ変数は元のメタ変数として報告される。

## メモ
