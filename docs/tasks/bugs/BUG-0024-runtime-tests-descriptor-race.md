# BUG-0024: ランタイムの単体テストが記述子番号の再利用で競合し、まれに失敗する

- 状態: done
- 重大度: minor
- 発見箇所: BUG-0022 の Docker CI (`cargo test` の `cirka_rt` の単体テスト)
- 関連: FEAT-0241, FEAT-0242

## 現象

`evloop::tests::watch_unwatch_and_a_reused_descriptor` がまれに失敗する。

```
panicked at crates/cirka-runtime/src/evloop.rs:1591:13:
assertion `left == right` failed
  left: 0
 right: -9
```

失敗したテストが共有のテストロック (`TEST_LOCK`) を持ったまま異常終了するのでロックが poison され、ロックを取るほかの 19 件も
`PoisonError` で失敗する (43 件中 20 件が失敗と表示される)。

## 再現手順

`cargo test -p cirka-runtime` を繰り返す (並列に走るテストの順序しだい)。

## 期待される動作

テストの結果が実行の順序に左右されない。1 件の失敗はその 1 件だけの失敗として報告される。

## 原因

このテストは「閉じた記述子の番号が次に開く記述子で再利用される」「閉じた記述子は監視できない」ことを確かめる。記述子の番号は
プロセスで共有なので、並列に走るほかのテストが間で記述子を開くと成り立たない。FEAT-0242 で加えた `os::tests` の 6 件
(パイプ・子プロセス・signalfd・ファイル) と `net::tests::descriptor_helpers` は、`TEST_LOCK` を取らずに記述子を開いていた。

## 修正

- [x] 記述子を開くテストはすべて `TEST_LOCK` を取る (`os::tests` の 6 件と `net::tests::descriptor_helpers`)
- [x] ロックの取得を `crate::test_lock()` にまとめ、poison されていても取れるようにする (1 件の失敗をほかへ広げない)

## メモ

- 回帰テストは置かない (テスト自身の修正)。修正後に `cargo test -p cirka-runtime` を 30 回続けて実行し、すべて通った。
