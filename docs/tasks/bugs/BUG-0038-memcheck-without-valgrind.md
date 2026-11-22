# BUG-0038: `scripts/memcheck.sh` が実行テストを valgrind の下で走らせていない

- 状態: in-progress
- 重大度: major
- 発見箇所: FEAT-0096 (新しいランタイムの経路を valgrind で確かめようとしたとき)
- 関連: FEAT-0004, FEAT-0013, FEAT-0096

## 現象

`scripts/memcheck.sh` は `CIRKA_ALLOC=system` と `CIRKA_VALGRIND=1` を設定して `cargo test` を走らせるが、
`CIRKA_VALGRIND` を読むコードがどこにもない (FEAT-0004 で雛形を作ったときから)。実行テストのネイティブの実行は
valgrind を通らず、システムアロケータで走るだけになっている。10-testing.md と AGENTS.md の
「`CIRKA_ALLOC=system` + valgrind でエラー 0」は、これまで個別のチケットで手で確かめた範囲 (FEAT-0013 のスモークテストなど) でしか
検査されていない。

## 再現手順

```sh
grep -rn CIRKA_VALGRIND crates/ tests/   # 何も出ない
scripts/memcheck.sh                      # valgrind のプロセスが起動しない
```

## 期待される動作

`CIRKA_VALGRIND=1` のとき、テストのネイティブの実行 (MIR / Core / aCHC / CirkaLang) を valgrind (memcheck) の下で走らせ、
不正な読み書き・未初期化の値の使用などの誤りが 1 件でもあればそのケースを失敗にする。リークはランタイムのリークチェック
(`CIRKA_LEAK_CHECK=1`) で調べているので valgrind では調べない。

## 原因

FEAT-0004 で `memcheck.sh` の雛形に環境変数を置いたが、テストの実行側 (tests/lib.rs、cirka-testkit) に valgrind で包む処理を
実装しなかった。

## 修正

- [x] `CIRKA_VALGRIND=1` のとき、ネイティブの実行を `valgrind -q --error-exitcode=97 --leak-check=no` で包む
  (`cirka_testkit::valgrind_command`。TSan の実行は除く。valgrind の誤りは専用の終了コード 97 と標準エラーの報告で判定する。
  テストが期待する終了コードは 0・1・3 だけ)。valgrind のコマンドは `.env` の `CIRKA_VALGRIND_BIN`
- [x] 回帰テスト (`tests/harness_selftest.rs`): 解放した領域を読む C のプログラムを、包む処理が誤りとして検出し、
  その読み込みをしない実行は通ること (valgrind か C コンパイラがなければ飛ばす)
- [ ] `scripts/memcheck.sh` を Docker 内で全テストに対して走らせ、見つかった誤りを直す (範囲外のものは別のバグチケットに)

## メモ
