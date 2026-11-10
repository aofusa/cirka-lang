# BUG-0008: newtype の構築子パターンが値を評価してしまう

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0119 (互換計測、GHC testsuite の codeGen/cgrun062)
- 関連: FEAT-0104, FEAT-0119

## 現象

```haskell
newtype T = C { f :: String }
main = print (case undefined of C {} -> True)
```

GHC は `True` を出力するが、aCHC は `Prelude.undefined` で終了する。

## 再現手順

```sh
scripts/compat.sh   # CIRKA_COMPAT_FILTER=codeGen/cgrun062
```

## 期待される動作

Haskell 2010 §4.2.3: newtype の構築子に対する照合は値を評価しない (パターンは中のパターンの照合と同じ)。
`case undefined of C _ -> True` は `True`、`case undefined of C x -> ...` は `x` が使われるまで評価しない。

## 原因

脱糖 (パターン照合のコンパイル) が newtype の構築子パターンを通常の構築子と同じく `case` で照合しており、
newtype は表現が中身と同じなので `case` が中身を評価する。

## 修正

- [x] 回帰テストを追加 (`tests/achc/diff/newtype_patterns.hs`。-O0 と -O2、出力は GHC と一致)
- [x] newtype の構築子パターンを、照合する値のキャストに対する中のパターンの照合として脱糖する (`achc-desugar` の `con_block`)
- [x] newtype の構築子をキャストとして簡約し、照合のキャストと相殺させる (`opt.rs` の `newtype_cast_type`、`cast_to`。単体テスト `newtype_constructors_cancel_with_casts`、`newtype_constructors_that_cannot_be_casts_stay`)
- [x] 値の評価に頼っていたライブラリを直す (`Cirka.Array.Unboxed.thaw`)
- [x] 互換計測の基準を更新する (`codeGen/cgrun062`: crash → pass)

## メモ

**脱糖。** `con_block` は newtype のとき `case` を作らず、`let f = cast v τ` で列を置き換えて中のパターンを照合する。評価は中のパターンが決める。
レコードの更新 (`undefined { f = "x" }`) も同じ経路を通るので直っている。Core の `case` は newtype の構築子でも被検査式を評価する (CirkaLang は正格なのでこれでよい)。

**性能への影響。** 構築子パターンの評価がなくなると、`Cirka.Array.Unboxed.thaw` の結果の配列がサンクのまま `steps` に渡り、ループの入口の検査に失敗して n-body `own.chs` が 227.4 M → 296.9 M 命令に悪化した (FEAT-0076 の冷たい経路の数字と同じ)。
`thaw` が結果を先に評価するように書き換えて元に戻した (`let !b' = primUArrThaw b in unique (UArray b')`)。

命令数 (callgrind Ir、修正前 → 修正後、`own.chs`): n-body 500k 227.44 M → 227.44 M、fannkuch-redux 9 114.18 M → 114.18 M、array-sort 1M 294.47 M → 294.47 M、spectral-norm 300 64.57 M → 64.58 M (+0.02%。`bytes_get` の順序が入れ替わった分)。`plain.hs` と CirkaLang の LLVM IR は同一。

**構築のキャスト化。** 照合が `cast` になると、これまで `case (con C x)` の相殺で消えていた newtype の構築子が箱として残る。
`tests/achc/run/h3/borrow_loop.hs` の確保回数は 60102 → 540102 に増えた (テストの上限は 100000)。
簡約器が newtype の構築子を `cast` に置き換えて、構築と照合のキャストを相殺するようにして元の 60102 に戻した。
相殺のあとに残る「構築子を自分の型へ移す `cast`」も落とす。残すと `case (cast (con I# x) Int) ..` の既知の構築子の簡約が起きない。

**互換計測のスタック。** `compat_child` はテストのスレッド (既定 2 MiB) でコンパイルしていて、`numeric/numrun014` が修正で限界を超えた (スタックあふれ)。CLI のスレッドと同じ 256 MiB で動かすようにした (`RUST_MIN_STACK`)。CLI では元から問題ない。

ミューテーション (すべて検出): 照合で評価し続けると `achc_diff` の `newtype_patterns` が失敗する。構築子を `cast` にしない、構築子を自分の型へ移す `cast` を残す、のどちらも `newtype_constructors_cancel_with_casts` が失敗する。
線形の newtype や unboxed のフィールドの newtype も `cast` にすると、Core Lint が拒否して `newtype_constructors_that_cannot_be_casts_stay` が失敗する。
