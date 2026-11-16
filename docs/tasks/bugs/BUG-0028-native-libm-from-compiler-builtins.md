# BUG-0028: ネイティブのプログラムが libm の一部の関数をランタイムの compiler-builtins から取る

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0252 (`Std.Math` のテストで、インタプリタとネイティブの出力が食い違った)
- 関連: FEAT-0252, FEAT-0047

## 現象

ネイティブのプログラムで、C の数学関数のうち `cbrt`・`fma`・`floor`・`ceil`・`round`・`trunc`・`rint`・`fmod`・`fmin`・`fmax`・`fdim`・
`copysign`・`fabs`・`sqrt` (と `f` 付きの版) の呼び出しが、glibc の libm ではなく `libcirka_rt.a` に入っている Rust の compiler-builtins の実装に解決される。

- `cbrt 27.0` がネイティブでは 3、インタプリタ (glibc の `cbrt` を呼ぶ) では 3.0000000000000004 になり、同じプログラムの結果が実行方法で変わる。
  glibc の `cbrt` は正しく丸めた結果を返すとは限らない (C の規格どおり) が、`link "m"` で宣言した関数は C ライブラリのものであるべきである。
- `fma` は compiler-builtins ではソフトウェアの実装になり、glibc (CPU の FMA 命令を使う) より遅い。
- aCHC の `Foreign` で宣言した同名の関数も同じ。

## 再現手順

```sh
cirka run tests/cirkalang/run/v006/libm_link.cirka   # 修正前は 1 行目が 3 (インタプリタは 3.0000000000000004)
```

## 期待される動作

ネイティブのプログラムの数学関数の呼び出しは、インタプリタと同じ C ライブラリ (glibc の libm) の関数になる。

## 原因

`cirka-driver` の `link_ir` のリンクの順序が `out.ll libcirka_rt.a <link の引数> -lm -lpthread` で、
プログラムの未定義の `cbrt` などがアーカイブを走査した時点で compiler-builtins のメンバー (弱いシンボルの定義) から解決されていた。
compiler-builtins は、libm のない環境のためにこれらの関数を弱いシンボルとしてすべての対象で提供している。

## 修正

- [x] 回帰テストを追加 (`tests/cirkalang/run/v006/libm_link.cirka`: `cbrt 27.0` などを表示し、ネイティブとインタプリタの一致を確かめる。
      修正前はネイティブとインタプリタの出力が食い違うことを確かめた)
- [x] 修正: ランタイムより前にも `-lm` を置く (`out.ll -lm libcirka_rt.a <link の引数> -lm -lpthread`)。
      共有ライブラリの定義が先に入るので、アーカイブの弱い定義は取り込まれない。07-backend.md §4 に追記

## メモ

- ランタイム自身 (Rust) の浮動小数点の剰余なども glibc の関数になる。結果は変わらない (いずれも正確な演算)。
