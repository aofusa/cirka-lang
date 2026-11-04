# FEAT-0205: cirka CLI と cirka.toml

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0204, FEAT-0031, FEAT-0032
- 設計: [01-architecture.md](../../design/01-architecture.md)

## 概要

`cirka build/run/check`、`cirka.toml`、`cirka --version` (v0.0.1)。`tests/cirkalang/{run,fail}`。

## 作業内容

- [x] CLI
- [x] `cirka.toml` の `[package]`
- [x] テスト種別の有効化

## 受け入れ条件

- [x] `tests/cirkalang/run/v001/` (fib, map, ADT, 再帰) がリーク 0 で動く
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `crates/cirka`: ライブラリ (`compile` / `frontend`) と CLI。CLI 本体・ダンプ・Core 以降は `cirka-driver` と共通。
  `--dump=rn,tt` はフロントエンドのダンプ (`cirka-driver` の `frontend_dumps`)。`cirka --version` は `CirkaLang v0.0.1-dev`。
- ライブラリの場所は `CIRKA_LIB_DIR` (`.env.sample` に追記) → `<bin>/../lib/cirkalang` → リポジトリの `lib/cirkalang`。
- `cirka.toml` は `[package]` の `name` / `edition` だけを読む最小のパーサ (外部クレートなし)。ディレクトリを渡すと `src/Main.cirka` を入口にする。
- コンパイラは深い再帰があるため 256 MB のスタックのスレッドで動かす (CLI・テストとも)。
- テスト: `tests/cirkalang.rs` に `cirkalang_run` (-O0/-O2 でインタプリタ実行・リーク 0・ネイティブ一致) と
  `cirkalang_type_errors` (`fail/type`, `fail/own` の診断の完全一致) を追加。
  `panic` / `exit` で終わるプログラムは巻き戻さないので、その場合だけリーク検査を外す。
- 実行テストで見つかって直したもの:
  - `do` の `e; rest` を `e >>= \_ => rest` に脱糖 (正格評価で `e >> rest` だと後続の引数が先に評価される。spec §5 を更新)。
  - 適用される未知の型のメタ変数を関数型 `?A -> ?B` に細分化 (`foldr (\f, acc => f acc)`)。
  - `let` 全体を「rig 回使われるスコープ」として検査 (ω の文脈の中の `let 1` を許す)。
  - 空の `case x of {}` (コンストラクタがすべて不可能な型)。
  - 資源を持たない型の引数は ω の位置でも 1 回と数える (ADR-030)。
  - `support.core` の文字列補助関数がバッファを引数で渡した後に `bytes_set` して共有オブジェクトを書き換えていた。結果としてバッファを返す形に変更。
