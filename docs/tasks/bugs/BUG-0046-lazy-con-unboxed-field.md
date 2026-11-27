# BUG-0046: 遅延束縛の構築子の unboxed フィールドが、`!` がないと束縛の時点で計算される

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-0040 (`tests/core/run/catch_strict.core` の最適化後の実行)
- 関連: BUG-0014, FEAT-0056

## 現象

フィールドを `!` なしで宣言した unboxed の構築子 (`(data Box () (B Int#))`) を遅延で束縛すると、要求されていなくても束縛の時点でフィールドを計算する。

```lisp
(let ((a Box (con B () (prim quot 7 n))))
  (case n (k Int#) Int# (0 3) (_ (case a (s Box) Int# ((B v) v)))))
```

`n = 0` で `a` を誰も要求しないのに、0 での除算で止まる (`!` を付けた `(B (! Int#))` では BUG-0014 の修正どおり 3 を返す)。
`div` をインライン化して `I# (quot 7 0)` を遅延で束縛した FEAT-0040 のテストでは、`catch` の中で起こるはずの例外が `catch` の前で起こった。

## 再現手順

```sh
CIRKA_TEST_FILTER=lazy_con_unboxed_field cargo test --test core
```

## 期待される動作

遅延の位置の構築子は、構築で計算するフィールド (正格なフィールドと unboxed のフィールド。unboxed の値はサンクにできないので、
宣言に `!` がなくても構築の時点で計算する) が今計算しても安全なときだけその場で作り、それ以外はサンクにする。

## 原因

MIR への下ろし (`lower.rs` の `builds_now`) と、評価済みの引数の判定 (`ww::is_value`、FEAT-0056) が、フィールドの宣言の `!` だけを見ていた。
`!` のない unboxed のフィールドを遅延のフィールドとして扱い、安全かを問わずにその場で作れる (評価済み) とした。
フロントエンド (aCHC の `prims.core`、CirkaLang) は unboxed のフィールドに `!` を付けるので、手書きの Core でだけ起こる。

## 修正

- [x] 回帰テストを追加 (`tests/core/run/lazy_con_unboxed_field.core`、MIR の単体テスト `lazy_constructors_wait_for_unsafe_unboxed_fields`)
- [x] 構築で計算するフィールドを `CtorDecl::computes` (正格、または unboxed の型) とし、`builds_now`・`ww::is_value`・正格性解析の構築子の扱いで使う

## メモ

ミューテーション (すべて検出): `builds_now` を `!` だけに戻すと `lazy_con_unboxed_field` (Core の実行) と MIR の単体テストが失敗する。
`ww::is_value` を戻すと、FEAT-0040 の `catch_strict` の最適化後の実行が失敗する (worker/wrapper が評価済みとして `catch` の前で箱を開ける)。
フロントエンドの出力は変わらない (unboxed のフィールドはすでに `!` 付き)。
