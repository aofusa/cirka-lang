# BUG-0031: 同じ呼び出しが証明義務の中で深さによって別の項に訳され、仮定と目標が一致しない

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0254 (`bench/stencil/main.cirka` の `U.thawS n (U.generate n f)`、`n = parseInt arg`)
- 関連: FEAT-0244, FEAT-0254

## 現象

`let n = parseInt arg` のあと `if n < 3 then ... else` の中で `U.thawS n (U.generate n f)` と書くと、`thawS` の `requires size arr == n` が
示せない (E0501。反例は `parseInt` の中身の値で分かりにくい)。`generate` の結果の大きさの契約 (`size r == (if n < 0 then 0 else n)`) と
経路の条件 (`n >= 3`) から明らかに成り立つ。`n` を関数の引数にすると示せる。

## 再現手順

```
import Data.Array.Unboxed as U

sizeOf : String -> Int
sizeOf arg =
  let n = parseInt arg in
  if n < 3 then 0
  else U.size (U.freezeS (U.thawS n (U.generate n (\i => toFloat i))))
```

## 期待される動作

同じ値 (`parseInt arg`) は、証明義務のどこに現れても同じ項に訳され、示せる。

## 原因

検証器は定義の呼び出しを展開して SMT の項に訳し、深さ 24 で打ち切って残りを不透明な変数にする。`parseInt` は再帰する `parseDigits` を
使い、文字列が未知なので展開は打ち切りまで続く。目標の `n` (深さ 2) と、`generate` の呼び出しの代わりの変数に付く仮定の中の `n`
(深さ 5) は打ち切る位置が違い、別の項になった (途中の添字も `0 + 1` と `1` で別の不透明な変数になった)。Z3 はそれらが等しいことを知らない。
FEAT-0244 の大きさの代わりの変数から起きていた。

## 修正

- [x] 回帰テストを追加 (`tests/cirkalang/verify/pass/parsed_sizes.cirka`。ベンチ `bench/stencil` も `bench_programs` で検証される)
- [x] 修正: 一つの証明義務の中で、定義の呼び出しの最初の訳を覚えて、同じ呼び出し (引用した値が同じもの) には同じ項を使う。
      訳している途中の呼び出しが自身の展開の中に現れたら不透明な変数にする。二つの訳が等しいという仮定を加える方法は、
      止まらない呼び出し (`f x = f x + 1`) で矛盾する (`t + 1 = t`) ので採らない (訳を共有するだけなら新しい仮定はない)

## メモ

- 最初の訳が深い位置で打ち切られていると、浅い位置で展開すれば示せた事実を失うことがある。既存のテストとライブラリの検証では起きなかった。
- ライブラリの検証の時間は変わらない (Docker で 3.1 秒)。
