# BUG-0001: ランタイムの単体テストが並列実行で時々異常終了する

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-0022 作業中の `cargo test -p cirka-runtime --lib`
- 関連: FEAT-0013

## 現象

`cargo test -p cirka-runtime --lib` が約 2 割の確率で `free(): invalid pointer` (SIGABRT) や SIGSEGV で異常終了する。

## 再現手順

```sh
cargo test -p cirka-runtime --lib --no-run
for i in $(seq 60); do target/debug/deps/cirka_rt-<hash> -q || echo fail; done
```

## 期待される動作

常に成功する。

## 原因

ランタイムの状態 (アロケータのフリーリスト等) は単一スレッド前提で、`tests.rs` のテストは Mutex で直列化していたが、
`alloc.rs` 内の単体テスト `alloc_free_reuses_memory` だけがロックを取らずに並列に走っていた。
生成コードの実行時には起きない (テストハーネス固有の問題)。

## 修正

- [x] 回帰テストを追加 (既存テストの繰り返し実行で確認: 60 回中 0 回失敗)
- [x] 修正: クレート共通の `TEST_LOCK` を設け、ランタイム状態に触れる全単体テストで取る

## メモ
