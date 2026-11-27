# BUG-0045: 演算子の名前を持つ関数・構築子の MIR のテキストが読み戻せない

- 状態: todo
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

## 修正

- [ ] 回帰テストを追加 (aCHC・CirkaLang の実行テストの最終の MIR の往復)
- [ ] 修正 (識別子でない名前を引用して表示し、読み込みで引用を受け付ける)

## メモ

- FEAT-0040 の計測は、MIR のテキストを経由せずに `--unwind` (アンワインドの様式のコードを作る) で行った。
