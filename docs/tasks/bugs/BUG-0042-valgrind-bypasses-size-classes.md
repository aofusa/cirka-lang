# BUG-0042: valgrind の下ではランタイムのアロケータが大きさのクラスを使わず、すべてを malloc で確保する

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-1001 (HTTP サーバの callgrind の計測)
- 関連: FEAT-0091, FEAT-1001

## 現象

callgrind で HTTP サーバを計測すると、要求ごとに `malloc` が 4 回・`free` が 4 回呼ばれ、それだけで命令の 1/4 になる。
要求ごとの確保は 56 バイト以下の小さいオブジェクトだけで、ネイティブではスレッドごとのフリーリストから取る。
`alloc.rs` の `init` は大きさのクラスのための 64 GiB の領域を `mmap` (`MAP_NORESERVE`) で予約するが、valgrind はこの大きさの
予約を断るので、ランタイムは黙ってシステムのアロケータ (`CIRKA_ALLOC=system` と同じ) に切り替わる。valgrind の計測
(callgrind・cachegrind) は、確保の多いプログラムでネイティブと違う確保の費用を見せる。`ulimit -v` で仮想メモリを制限した環境でも同じ。

## 再現手順

```sh
valgrind --tool=callgrind docs/artifacts/f1001/prof/http-new 0   # 負荷をかけると raw_alloc_slow → malloc が確保の数だけ呼ばれる
```

## 期待される動作

予約できる範囲で大きさのクラスを使う (valgrind の計測がネイティブの確保の費用を表す)。

## 原因

`init` は 64 GiB の予約だけを試し、失敗するとシステムのアロケータに切り替える。

## 修正

- [x] 回帰テストを追加 (`a_smaller_region_when_the_large_one_is_refused`: 予約の関数に、大きな予約を断る写像を渡す)
- [x] 修正: 64 GiB が断られたら半分ずつ小さくして 1 GiB まで試す (`reserve`)。領域の大きさを状態に持ち、ページの上限と
  解放のときの範囲の判定に使う

## メモ

- 修正の後の HTTP サーバの callgrind (FEAT-1007 のメモ) には、要求ごとの `malloc` / `free` がない (修正の前は要求ごとに 552 命令)。
