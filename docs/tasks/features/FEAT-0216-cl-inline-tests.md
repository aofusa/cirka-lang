# FEAT-0216: ソース内のテスト (`test` 宣言と `cirka test`)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P1
- 依存: FEAT-0207
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md) §11、ADR-048

## 概要

Rust の `#[test]` / `#[cfg(test)]` のように、プログラムの本体とテストを一つのソースファイルに同居させる。
FEAT-0132 のレビューでの要望。

## 作業内容

- [x] 構文: `test "名前" = 式` (文脈キーワード) と `tests where { 宣言 }` ブロック (cirkalang-syntax)
- [x] `build` / `run` では構文解析の直後にテストを捨てる。`check` と `test` では残す
- [x] 名前の重複 (E1112)、テスト専用の名前をテスト以外から使う誤り (E1113)
- [x] テスト用の `main` を生成する (引数のテスト番号でテストを選ぶ。`main` は不要)
- [x] `cirka test [--filter=s] [path]`: 実行ファイルを作り、テストごとに別プロセスでリークチェック付きで走らせ、結果と集計を出す
- [x] Prelude に `assert`・`assertEqInt` など
- [x] spec と 12-cirkalang.md §11 を実装に合わせる

## 受け入れ条件

- [x] 成功・失敗 (assert・panic・exit・リーク) の各場合を確かめる統合テスト (`tests/cirkalang/test/`)
- [x] `cirka build` の生成物にテストが残らないことを確かめるテスト
- [x] E1112 / E1113 の fail テスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装: 構文 (`cirkalang-syntax` の `DeclKind::Test`/`Tests`)、展開と除去 (`cirkalang_syntax::testing`)、テストの収集・E1113・
  テスト用の `main` (`main$test`。`cirkalang-resolve` の `TestMode`)、共通ドライバの `test` コマンド (`SourceTests`、`run_tests`)。
  テストは `test$N : IO ()` という非公開の定義になり、テスト用の `main` は resolve 後の rast で作る (他のモジュールの非公開の定義も参照できる)。
- `cirka test` は `exe --list` で一覧を得て、`exe N` をテストごとに `CIRKA_LEAK_CHECK=1` で実行する。既定のランタイムは debug。
  panic・`exit` で終わったときに生きているオブジェクトはリークとして扱わない (05-runtime.md と同じ)。
- テスト: `tests/cirkalang/test/` (`driver` の `cirka_source_tests`)、`tests/cirkalang/fail/test/` (E1112/E1113、check のモード)、
  `tests/cirkalang/run/v0.0.2/tests_stripped.cirka` (型の誤りのあるテストがあっても build で捨てられる。`cirkalang_run` は Core に
  `test$` が残らないことも確かめる)、構文・展開・収集・リーク判定のユニットテスト。リークの失敗は安全なコードでは作れないため、判定関数 `test_failure` のユニットテストで確かめた。
- `do` の最後の文に `exit` を書くと型エラーになる既存の不具合を BUG-0013 として起票した (テストでは注釈で回避)。
- aCHC の `achc test` は「対応しない」診断で終了コード 1。
