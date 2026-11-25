# FEAT-1007: I/O のシステムコールを libc を通さずに呼ぶ

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1001, FEAT-0091
- 設計: [05-runtime.md](../../design/05-runtime.md) §10.3

## 概要

プログラムは大きなスタックを持つ専用のスレッドで動く (05-runtime.md §7) ので、プロセスは常にマルチスレッドである。
glibc の `read`・`write`・`send`・`recv`・`accept4`・`epoll_wait` は取り消しの点 (cancellation point) で、マルチスレッドのプロセスでは
呼ぶたびに `__pthread_enable_asynccancel` / `__pthread_disable_asynccancel` (アトミックな比較交換が 2 回) を挟む。ランタイムは
スレッドの取り消しを使わない。HTTP サーバの callgrind では要求ごとに 64 命令 (Rust のサーバは 1 スレッドなので glibc がこれを省く)。

イベントループとソケット・パイプの I/O のシステムコールを、`syscall` 命令で直接呼ぶ (返り値と `errno` の約束は libc と同じにして、
呼び出し側を変えない)。ThreadSanitizer のランタイム (`--runtime=tsan`) は libc の関数のまま: その横取りが記述子を通した
スレッド間の happens-before を作るため。

## 作業内容

- [x] 設計: 05-runtime.md §10.3
- [x] `sys.rs`: `read`・`write`・`send`・`sendto`・`recvfrom`・`accept4`・`epoll_wait`・`epoll_ctl` を `syscall` 命令で実装する
  (`cirka_tsan` の構成では libc)。`build-runtime.sh` の tsan の構成に `--cfg cirka_tsan`
- [x] テスト: 生のシステムコールの返り値と `errno` (成功・`EAGAIN`・`EBADF`)、既存のイベントループ・ソケットのテスト (TSan を含む)
- [x] HTTP サーバの命令数と CPU 時間を計測して記録する

## 受け入れ条件

- [x] HTTP サーバの要求ごとの `__pthread_enable_asynccancel` がなくなる
- [x] 既存のテスト (TSan の構成を含む) が通る
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ
- ストリームの受信を `read` から `recvfrom` に変える案は取り下げた: `cirka_sock_recv` はパイプの記述子オブジェクトにも使われ
  (イベントループの単体テスト)、`recvfrom` は `ENOTSOCK` で失敗する。カーネルの経路の差 (ファイルの層) は数十 ns で、要求ごとの
  CPU 時間 (約 39 µs) に比べて小さい。
- 計測 (2026-11-25、i5-3320M。HTTP サーバに 16 接続で 2 万要求の callgrind と、20 万要求を 5 回交互に実行した CPU 時間の中央値。
  「前」は FEAT-1001〜1005 の木、「後」は FEAT-1006・1007 と BUG-0042 を加えた木):
  - 要求ごとの `__pthread_enable_asynccancel` / `__pthread_disable_asynccancel` (64 命令) と `cirka_force` (21 命令、FEAT-1006) が
    なくなった。BUG-0042 の修正で valgrind の下でも大きさのクラスを使うので、要求ごとの命令数は「後」で 1,405 (Rust 668)。
    「前」の 2,063 はシステムのアロケータの `malloc` / `free` (552 命令) を含む。
  - 要求ごとの user の CPU 時間: 16 接続で 5.85 → 5.50 µs、64 接続で 5.80 → 5.35 µs (Rust 3.45・3.50 µs)。sys の時間
    (約 35〜40 µs) は実行ごとのばらつきが差より大きい。
