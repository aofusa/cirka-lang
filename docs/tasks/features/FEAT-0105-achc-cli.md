# FEAT-0105: achc CLI と achc.toml

- 状態: done
- トラック: aCHC
- マイルストーン: H1 Haskell 最小フロントエンド
- 優先度: P0
- 依存: FEAT-0104, FEAT-0031, FEAT-0032
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

`achc build/run/check`、`achc.toml`、aCHC 用の run/fail テストの有効化。

## 作業内容

- [x] フロントエンドと共通ドライバの接続
- [x] `achc.toml` の `[package]`/`[compiler]`
- [x] `tests/achc/run`・`tests/achc/fail`

## 受け入れ条件

- [x] `tests/achc/run/h1/` (fact, fib, map, 遅延リスト, 文字列出力) が GHC と同じ出力・リーク 0
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `crates/achc/src/lib.rs`: `compile` (rename → typecheck → desugar) と共通ドライバのフロントエンド関数。`.hs` / `.chs`、
  `achc.toml` を含むディレクトリ (または `achc.toml` そのもの) を受け付ける。`--dump=rn,tc` で改名後の main モジュールと型付きプログラムを出力。
  基本ライブラリの場所は `ACHC_LIB_DIR` (空なら実行ファイルからの相対 `../lib/cirka-base`、なければリポジトリの `lib/cirka-base`)。
- `crates/achc/src/package.rs`: `achc.toml` の `[package]` (`name` / `main` / `source-dirs`)、`[compiler]`
  (`ownership` / `cycles` は値の検査のみ。所有権検査は H3)、`[dependencies]` は `cirka-base` のみ。誤りは E0002。
  `default-extensions` は未対応 (空以外はエラー)。
- 基本ライブラリ: `lib/cirka-base/{Cirka/Prim.hs, Prelude.hs, Data/Char.hs}` (09-libraries.md §1.2 の「H1 時点の範囲」)。
- テスト: `tests/achc/run/h1/{fact,fib,map,lazy,strings}.hs` を -O0 / -O2 の MIR インタプリタとネイティブ (debug / release
  ランタイム) で実行し、`.stdout` と一致・リーク 0。`CIRKA_GHC` があれば GHC の出力・終了コードとも一致することを確かめる
  (Docker イメージ内)。警告は `.warn` と比較。`tests/achc/pkg/hello` で `achc.toml` のパッケージを確認。
- 修正: 遅延パターン `~(a, b)` の 2 つ目以降の選択子が先の変数の束縛を上書きしていた (脱糖)。内包表記・`do`・レコード更新で
  生成した失敗節に W0611 が出ていた (型検査器で生成節の位置を消す)。

