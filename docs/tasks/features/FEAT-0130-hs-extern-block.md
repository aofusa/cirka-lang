# FEAT-0130: extern "C" ブロック (aCHC)

- 状態: done
- トラック: aCHC
- マイルストーン: H4 FFI 拡張と評価
- 優先度: P1
- 依存: FEAT-0116, FEAT-0122, FEAT-0047
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

`extern "C"` 構文、`OwnedPtr`/`BorrowedPtr`/`Nullable`、`free with`。

## 作業内容

- [x] 構文と脱糖: `extern "C" [link "lib"] where` とレイアウトのブロック (`CirkaExtern`、`.chs` で既定)、不透明型 `type T`、
      `f, g :: 型 [free with h]`。脱糖は `Maybe p` (`Nullable p`) を NULL ⇔ `Nothing` に変換し、`BorrowIO r t` / `Borrow r t` の結果を `IO` の
      ラッパの cast で作る。`link` はリンクするライブラリに加わる
- [x] C ABI 型の検査: C に渡せない型は E0618。型検査器が型の別名を展開した型で、`OwnedPtr` の引数の線形性・`OwnedPtr` を返す関数が `IO` で
      あること・借用ポインタの領域が結果の借用モナドの領域であることを確かめる (E0619)
- [x] BorrowedPtr への r の補完: 名前の解決の後に `BorrowedPtr T` に領域 `r` を補い、結果を `BorrowIO r` / `Borrow r` に持ち上げる。
      結果の `BorrowedPtr` も同じ領域。`withBorrowed` / `withBorrowedIO` の外に出すと E0320
- [x] `free with`: `Release (OwnedPtr T)` を生成する (`T` は同じブロックの不透明型、`T` ごとに 1 つ)
- [x] `achc bindgen --extern`: 関数と不透明型を extern ブロックで書く出力 (08-ffi.md §6 で本チケットに割り当てたもの)

## 受け入れ条件

- [x] `FILE*` を RAII で扱う例が動き、閉じ忘れが型エラー
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: 08-ffi.md §3・§4・§6・§7.4、ADR-101。型は `Unsafe.Extern` で定義し、`Cirka.Extern` が構成子を除いて再公開する。
- 実装: 構文解析 (`achc-syntax`) はブロックを `data T` と `foreign import ccall` に展開して印 (`ExternAttrs`) を付ける。名前の解決
  (`achc-rename/src/extern_block.rs`) が領域の補完と `free with` のインスタンスの生成、型検査 (`achc-typeck/src/externs.rs`) が型の規則、
  脱糖 (`achc-desugar/src/ffi.rs`) が NULL と借用モナドの変換を行う。
- RAII の規則の追加 (03-type-system.md §3.1): `case` の選択肢の右辺が `IO` の `do` ブロックなら、パターンが束縛した `Release` だけの値を
  そのブロックの終わりで解放する (`Just f -> do …`)。前は E0304 だった。`Drop` を持つ値は従来どおり右辺の先頭で落とす (ブロックは
  何度も実行されうる動作なので)。捕捉した資源を持つ動作を二度実行する形 (変数に束縛して二度・`replicateM_`・制限のない引数) は E0303 になることを確かめた。
  ガード付きの選択肢は従来どおり E0304。
- `OwnedPtr` を返す C の関数は `IO` に限った (純粋な呼び出しは共有・CSE・let の浮上で資源が複製されうる)。型の別名で書いた
  `BorrowedPtr r T` は補完をすり抜けて結果を借用の外に出せたので、展開後の型で領域を確かめる。
- テスト: `tests/achc/run/h4/extern_file.chs` (`FILE*` の `fopen`/`tmpfile` と `free with fclose`、`withBorrowedIO`、NULL)、
  `extern_raii.chs` + `.c` (生きている資源の数と閉じた順を C で数える: ブロックの終わり・`case` の選択肢の終わり・明示的な解放・
  NULL の引数・借用ポインタの結果)。`tests/achc/fail/compile/` (本チケットで加えた、`lib/cirka-base` を使う診断のテスト):
  閉じ忘れ E0302、借用の脱出 E0320、C に渡せない型 E0618、ブロックの誤り E0619 (呼び出し規約・`free with`・型の別名)。
  `tests/bindgen/gen/extern.h` と `tests/bindgen/run/counter/` (`--extern`)。
