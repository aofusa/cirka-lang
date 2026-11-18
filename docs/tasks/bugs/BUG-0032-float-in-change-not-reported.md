# BUG-0032: float-in が正格な束縛を遅延にした変更を報告せず、死んだ組が一意な配列を共有にする

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0253 (v0.0.6 のリリースのベンチ、`bench/array-sort`)
- 関連: FEAT-0255 (2b3434f で表に出た), FEAT-0064, FEAT-0065, FEAT-0253

## 現象

`bench/array-sort` (一意な unboxed 配列の in-place のクイックソート) が要素数の 2 乗の時間になった。
n = 16,000 で 1.9 秒 (v0.0.5 は n = 1,000,000 で 0.096 秒)。n = 2,000 で配列の複製が 14,338 回、確保が 159,767 回。
結果は正しい。

## 再現手順

```sh
cirka build -O2 -o as bench/array-sort/main.cirka
CIRKA_STATS=1 ./as 2000     # array_copies が 0 でない
```

## 期待される動作

一意な配列への書き込みは in-place で、複製しない (v0.0.5 と同じく線形の時間)。

## 原因

- `case partition u lo hi of (u, m) => …` の被検査式は `scrut$` に束縛され、インライン化と case-of-known-constructor の後に
  `let! scrut$ = MkPair u v` (死んだ正格な束縛) が残る。simplify は死んだ正格な束縛を除かない (`MkPair` のフィールドは正格で、
  構築は `u` を評価する。`u` はサンクかもしれないので、除くと発散を消しうる)。
- float-in (`strict::float_in`) はアトムだけのコンストラクタの正格な束縛を遅延 `let` にする (値なので同じ。FEAT-0065) が、
  これを変更として数えていなかった。パイプラインは float-in が 0 を返すと simplify を走らせないので、遅延になった死んだ束縛
  (simplify なら除く) が最後まで残る。
- MIR では、残った組の構築 (`alloc` や、正格フィールドの引数が評価済みと分からないときの `mkthunk`) が配列への 2 つ目の参照になる。
  Perceus は内側の `qsort` の呼び出しの前に `dup` を置くので、呼び出しの間ずっと配列が共有され、書き込みのたびに複製される。
- 2b3434f (FEAT-0255) より前は、ライブラリの使われないコードが Core の最適化の終わりまで残っていて、float-in がそこで束縛を動かし
  (0 でない値を返し)、simplify が毎回走っていた。2b3434f が最初の simplify の後に使われないコードを除くようにしたので、
  小さなプログラムでは float-in が 0 を返すようになり、隠れていたこの不具合が表に出た (bisect: da6ffb9 は良、2b3434f が最初の悪)。

## 修正

- [x] 回帰テストを追加: `cirka-core` の `opt_removes_dead_constructor_bindings_float_in_made_lazy` (修正前は死んだ `SP` の束縛が残る)、
      `tests/cirkalang/run/v006/uarray_quicksort.cirka` (`-- max-array-copies: 0`。修正前は複製 14,338 回)
- [x] 修正: float-in はコンストラクタの束縛を遅延にしたことも変更として数え、パイプラインはその後に simplify を走らせる (06-ir.md §1.5)

## メモ

- 修正後: `bench/array-sort` の n = 1,000,000 は 0.09 秒 (ホスト)、n = 2,000 の確保は 9 回、複製は 0 回。
- 他のパスの数え方も確かめた: strictify・speculate・特殊化 (`spec`・`sat`) は変えた束縛をすべて数える。
- MIR には、使われずに解放されるだけの確保 (`alloc` の直後の `drop`、`mkthunk` の直後の `drop`) を除く最適化がない。Core が
  死んだ束縛を残さない限り問題にならないが、残ると今回のように Perceus の `dup` が呼び出しをまたいで一意性を壊す。
