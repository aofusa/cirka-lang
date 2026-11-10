# BUG-0014: 遅延束縛の構築子の正格フィールドが束縛の時点で評価される

- 状態: done
- 重大度: major
- 発見箇所: BUG-0008 の修正中 (`tests/achc/run/h3/bounds_overrun.hs` の失敗)
- 関連: BUG-0008

## 現象

```haskell
data S = S !Int !Int
main = do
  let n = length ""
      s = S (10 `div` n) n
  putStrLn "hello"
  print (if n > 0 then (case s of S a _ -> a) else 0)
```

GHC は `hello` と `0` を出力するが、aCHC は `-O0` で `divide by zero` で終了していた (`s` が要求されていないのに `S` を作った)。

## 再現手順

```sh
target/release/achc build -O0 -o /tmp/s tests/achc/diff/strict_field_lazy_let.hs && /tmp/s
```

## 期待される動作

遅延位置の構築子は要求されたときに作る。正格フィールドの評価もそのときに行う (Haskell 2010 §4.2.1)。

## 原因

MIR への下ろし (`lower.rs` の `lazy`) が、遅延位置の `Con` を常にその場で作っていた。構築は正格フィールドを評価するので、失敗しうる式や呼び出しが束縛の時点で実行された。

## 修正

- [x] 回帰テストを追加 (`tests/core/run/lazy_con_strict_field.core`、`tests/achc/diff/strict_field_lazy_let.hs`、MIR の単体テスト)
- [x] 正格フィールドが今計算しても安全なときだけその場で作り、それ以外はサンクにする (`Lowerer::builds_now`)

## メモ

**安全な式。** 値・定数・ラムダ、失敗しないプリミティブ (`total_prim`) を安全な式に適用したもの。変数は、同じ関数の中で作ったサンクだけを危険とする。

**残した穴。** 仮引数は評価済みかどうか分からない場合でも安全とみなし、これまでどおり構築の時点で `force` する。サンクを渡された仮引数を遅延の構築子の正格フィールドに入れると、要求される前に評価される可能性が残る。
CirkaLang は正格なのでこの評価は常に正しいが、呼び出しの事実から評価済みと示せない仮引数があり、一律にサンクにすると n-body `main.cirka` が 592.5 M → 757.3 M 命令に悪化した (+28%)。
仮引数の評価済みの事実を正確にできれば (FEAT-0078 のメモの正格フィールドの検討を参照)、この穴も閉じられる。

**性能への影響。** spectral-norm `plain.hs` は命令数が 50.7 M → 65.7 M (+29.6%、n=100)、時間が 0.429 s → 0.631 s (n=1000。GHC -O2 は 0.540 s、Rust は 0.174 s)。ほかのベンチは変化なし (±0.04%)。
リストの要素 `dot (a i) u` をサンクにするようになったためで、意味論の修正に必要なコストである。修正前に GHC より速かったのは、呼び出しを要素の束縛の時点で実行していたからで、正しくなかった。
GHC も同じサンクを作る (`-ddump-stg-final` で要素ごとの `\u []` を確認)。

ミューテーション (すべて検出): `builds_now` を常に真にすると `core_run` の `lazy_con_strict_field` が失敗する。`safe_now` で変数を常に安全とすると `-O0` の `achc_diff` の `strict_field_lazy_let` が失敗する。
プリミティブをすべて安全 / すべて危険とすると MIR の単体テスト `lazy_constructors_wait_only_for_unsafe_strict_fields` が失敗する。
