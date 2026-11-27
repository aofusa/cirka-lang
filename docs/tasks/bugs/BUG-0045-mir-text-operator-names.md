# BUG-0045: 演算子の名前を持つ関数・構築子の MIR のテキストが読み戻せない

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-0040 (アンワインドの様式の費用の計測で、ベンチの `--dump=mir-opt` を `cirka-mir build` で読もうとした)
- 関連: FEAT-0014, FEAT-0047

## 現象

aCHC・CirkaLang のプログラムの最終の MIR (`--dump=mir-opt`) を `cirka-mir` で読むと、演算子の名前で止まる。

```text
error[M0001]: unexpected character `+`        fn Cirka.Base.++(a: ptr @owned, a: ptr @owned) -> ptr {
error[M0001]: unexpected character `!`        fn Cirka.Base.!!$w(a: ptr @owned, a: i64) -> ptr {
error[M0002]: expected `=`, found `:`         type Prelude.List { Prelude.Nil = 0 {}, Prelude.:: = 1 { ... } }
```

## 再現手順

```sh
cirka build -O2 --dump=mir-opt --build-dir=_build/x bench/spectral-norm/main.cirka
cirka-mir check _build/x/main.opt.mir
```

## 期待される動作

MIR の表示は、フロントエンドが作るすべての名前 (演算子の記号を含む関数・型・構築子・大域) を読み戻せる形で書き、
`print → parse → print` が変わらない (`core_mir_roundtrip` と同じ性質を aCHC・CirkaLang のプログラムにも)。

## 原因

`print.rs` は名前をそのまま書き、`parse.rs` の識別子は英字・数字と `_ $ % ' #` (と区切りの `.`) だけを受け付ける。
`tests/core/run` の Core のプログラムには演算子の名前がないので、往復の試験では見つからなかった。
直すと、ほかに 2 つの原因が出た。

- 構築子の経路 `Type.Ctor` を最後の `.` で分けていたので、名前に `.` を含む構築子 (`Prelude.List` の `Prelude.Nil`) を引けない。
- フロントエンドは同じ関数の別の変数・ブロックに同じ名前を付ける (`fn Cirka.Base.++(a: …, a: …)`) が、読み込みは名前で変数を区別する。

## 修正

- [x] 回帰テストを追加: `tests/driver.rs` の `optimized_mir_of_both_frontends_reads_back` (spectral-norm・fannkuch-redux の `plain.hs` と
      spectral-norm・cli-grep の `main.cirka` の最終の MIR を読み戻し、検証器を通し、表示し直して同じ文字列になる)、MIR の単体テスト
      `operator_and_dotted_names_round_trip`
- [x] 修正: 識別子でない名前を逆引用符で囲んで表示し、読み込みで受け付ける。構築子・フィールドの経路は分け方を順に試して宣言にある組を選ぶ。
      同じ名前の変数・ブロックは 2 つ目から `name%k` で区別して表示する (06-ir.md §2.1)

## メモ

- FEAT-0040 の計測は、MIR のテキストを経由せずに `--unwind` (アンワインドの様式のコードを作る) で行った。
- 修正を戻すと、回帰テストは ``unexpected character `+` `` で失敗する。
- 読み込みで作ったモジュールは名前が互いに異なるので、表示は変わらない (既存の往復の試験 `roundtrip_property`・`core_mir_roundtrip`)。
