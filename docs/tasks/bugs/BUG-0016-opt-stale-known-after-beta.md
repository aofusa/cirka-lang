# BUG-0016: β 簡約の後に既知の構築子の表が古い引数を指す

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0220 (`cirka-core` の単体テスト `state_token_needs_a_newtype_for_other_states`、デバッグビルドの Core Lint)
- 関連: FEAT-0220, BUG-0002

## 現象

インライン化した関数 `λn. let t = I# n in λx. t` を `3` に適用すると、`case` が既知の構築子の表から `n` を取り出し、
スコープにない変数 `n` が残る (デバッグビルドで Core Lint の C0100、リリースビルドでは不正な Core のまま進む)。

```
Core Lint failed after pass `simplify` in `main`: variable `n` (#13) is not in scope
(let ((t Int (con I# () 3))) (let ((x Int (con I# () 1))) n))
```

## 再現手順

```sh
cargo test -p cirka-core state_token_needs_a_newtype_for_other_states
```

## 期待される動作

`case t of I# v -> v` が `3` になる。

## 原因

インライン化 (`callee`) はラムダの本体を先に簡約し、そのとき `let t = I# n` を既知の構築子として記録する。
その後の β 簡約は、アトムの引数を簡約済みの本体に `replace_var` で代入するだけで、表の中の `n` は置き換えなかった。

## 修正

- [x] 回帰テストを追加 (`state_token_needs_a_newtype_for_other_states`)
- [x] 修正: β 簡約でアトムを代入するとき、既知の構築子のフィールドも置き換える。非アトムを 1 か所に代入するときは、その変数を含む記録を捨てる

## メモ
