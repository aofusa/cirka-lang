# FEAT-0244: core の配列・文字列の関数に大きさの契約を付ける

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P2
- 依存: FEAT-0235
- 設計: [spec-v0.0.4.md](../../design/cirkalang/spec-v0.0.4.md) §3, §5

## 概要

v0.0.4 の評価 (FEAT-0235) で、実行時に作った配列の大きさが検証から見えないため、`requires A.size xs == A.size ys` のような契約を
呼ぶ側で `if` で確かめ直す必要があった。`Data.Array.generate` / `new` / `fromList` / `map` / `thaw` / `freeze`、`Data.String` の `slice` などの結果に
大きさの `ensures` を付け、利用者のコードで境界の証明が通りやすくする。

## 作業内容

- [x] 結果の大きさの契約 (`ensures \r => size r == n` など。`n < 0` のときの扱いを決める)
- [x] 線形な値 (`Unique (Array a)`) の部分集合型、またはプリミティブの仕様を仮定として与える仕組み (信頼する仕様。`unsafe` と同じくライブラリに閉じる)
- [x] 検証済みサンプル (`sample_array.cirka`) から呼ぶ側の `if` を外せることを確かめる

## 受け入れ条件

- [x] `A.generate n f` の結果に対する添字 `0 <= i < n` の `A.index` が証明される
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- v0.0.4 では `LType` の値の部分集合型を扱わない (spec-v0.0.4.md §3.5) ので、一意な配列を作る関数には直接書けない。
- 設計 (spec-v0.0.5.md §4、ADR-091): 一意な配列を経て作る `generate`・`fromList` の大きさは **信頼する仕様 `trusted`** で与える。
  `trusted` は結果の部分集合型 (`ensures`) と `total` を示さずに仮定させ、`prim` と同じくライブラリ (`core`・`std`) の中だけで書ける
  (E1009。BUG-0023 と同じ検査)。`map` の契約は `generate` の契約から示す (信頼しない)。
- `n < 0` の `generate` は空の配列 (v0.0.2 からの動作): `ensures \r => size r == (if n < 0 then 0 else n)`。`f` は部分集合型
  `{i : Int | 0 <= i && i < n}` の添字を受け取る。
- ライブラリの証明義務は利用者のコンパイルでは調べず (Z3 なしでライブラリを使える)、テスト `cirkalang_verify_libraries` が
  `--libraries=verify` ですべてのモジュールを検査する。義務を 1 つ示せなくすると失敗することを確かめた (`foldrFrom` の `requires` を外す)。
- 検証器: 評価で整数になる関数の呼び出し (`Prelude.length [4, -1, 3, 7, -2, 5, 1]`) をその値として扱う (`Num` の `+` を経る
  `length` は定義の展開では深さの上限で止まり、`maxWindow (A.fromList [...]) 3` が示せなかった)。反例は大きさの仮定を使った値を
  呼び出しの大きさとして示す。
- テスト: `tests/cirkalang/verify/pass/array_sizes.cirka` (`squareAt`: `A.generate n f` の添字 `0 <= i < n`、`reversed`・`lastDoubled`・
  `third`・`generated`)、`verify/fail/array_size_off_by_one.cirka`、`run/v005/array_sizes.cirka` (仕様を実行時に確かめる)、
  `fail/type/trusted_outside_library.cirka`、`fail/syntax/modifier_twice.cirka`、構文の `parse/totality.cirka`、
  検査器の単体テスト 2 つ、`verify/pass/sample_array.cirka` から呼ぶ側の `if` を外した。
- 計測 (ホスト、10 回の平均): ライブラリの内部の繰り返し (`get`・`toList`・`map`・`foldl`・`foldr`・`==`) も `index` で読むように
  して試した。100 万要素の配列の `foldl`+`foldr` 100 回は 602.9 ms (証明) と 606.2 ms (`--contracts=off`、検査あり) で差がない
  (LLVM -O3 が繰り返しの条件から検査を消していた)。`map`+`==` 100 回は 4743.7 ms と 4605.1 ms で、証明側が速くならない
  (panic の呼び出しは 21 か所から 16 か所に減るが、箱の確保が支配的)。効かないので入れなかった (16-performance.md §4)。
- 一意な配列 (`new`・`thaw`・`write`) の大きさと、文字列のビュー (`Data.String`) の大きさの契約・証明済みの読み出しは
  FEAT-0254 に分けた (`LType` の値の大きさを証明で追う仕組みが要る)。
