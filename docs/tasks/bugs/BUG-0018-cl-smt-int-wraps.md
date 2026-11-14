# BUG-0018: 検証が `Int` を数学の整数として扱い、ラップする演算で偽の証明が通る

- 状態: done
- 重大度: critical
- 発見箇所: FEAT-0235 (リリース前の確認)
- 関連: FEAT-0230, FEAT-0232, FEAT-0234

## 現象

SMT への変換が `Int` を数学の整数として扱っていた。`Int` は 64bit で、演算はオーバーフローでラップする (spec-v0.0.1.md)。
そのため、実行時には成り立たない性質が証明され、FEAT-0234 の検査除去と組み合わさると **メモリ安全が破れる**。

```
get arr i k = if 0 <= i && 0 <= k && i + k < A.size arr then A.index arr (i + k) else 0
```

`i = k = 2^62` で `i + k` は負にラップし、`i + k < size` は成り立つので、境界検査のない `index` が範囲外を読む。
同じく `n * 2 == 0 && n /= 0` (最小の `Int` で成り立つ) の分岐を `unreachable` にでき、`abs b` (最小の `Int` で負) を尺度にできた。

## 再現手順

```sh
cirka run tests/cirkalang/verify/fail/overflow.cirka   # 修正前は検証を通った
```

## 期待される動作

ラップする演算の結果について、実行時に成り立つことだけが証明される。

## 原因

`measure.rs` の変換が `+ - * /` と符号反転をそのまま SMT-LIB の整数演算にしていた。変数にも範囲がなかった。

## 修正

- [x] 回帰テストを追加 (`tests/cirkalang/verify/fail/overflow.cirka`、`cirka-smt` の `z3_wrapping_arithmetic`)
- [x] 修正: `Int` の `+ - * /` と符号反転の結果を 64bit にラップする (`Expr::wrap`: `((e + 2^63) mod 2^64) - 2^63`)。
  `%` はオーバーフローしない。`Int` の変数と値の分からない式は `[-2^63, 2^63 - 1]` にあると仮定する (`Query::bound_int_vars`)。`Nat` の変数は上限なし。

## メモ

- 数学の整数では成り立っても `Int` では成り立たない性質を使っていたサンプル (`verify/pass` の refine / contracts / measures) は、範囲を絞るように書き直した
  (`succPos : {n : Int | n < 1000000} -> {r : Int | r > n}`、`fib n >= 0` の帰納法は `fib 93` がラップするので偽。`parity` の例に変えた)。
- binary search (`lo + (hi - lo) / 2`) やサンプルの境界の証明は、0 <= lo <= hi <= size からオーバーフローしないことが示せるので、そのまま通る。検証時間もほぼ変わらない。
