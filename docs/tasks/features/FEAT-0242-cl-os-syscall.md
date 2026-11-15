# FEAT-0242: OS 層 (syscall・シグナル・プロセス)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P1
- 依存: FEAT-0213
- 設計: [09-libraries.md](../../design/09-libraries.md)

## 概要

libc 経由の syscall ラッパ、errno → Result、シグナル、子プロセス、mmap。

## 作業内容

- [x] std.os (`Std.Os`。子プロセスは `Std.Process`、ディレクトリと属性は `Std.Fs`、作業ディレクトリは `Std.Env` に置いた。spec-v0.0.5.md §3)
- [x] errno の Result 化 (`cResult`・`cResultLong`・`cResultSize` とエラー番号の定数、§3.1)
- [x] シグナル・プロセス・mmap (ランタイム `os.rs`、05-runtime.md §11)

## 受け入れ条件

- [x] syscall を使う CUI ツールの run テスト (`tests/cirkalang/run/v005/os_tool.cirka`。サブコマンド `tree`・`lines`・`run`・`signals`
      と、引数なしの自己テスト)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

**設計 (ADR-090).**

- 子プロセス `Child` は線形な値で、`Release` が終わりを待つ。取り出していないパイプを閉じてから待つので、入力を待つ子も終わる。
  回収を忘れられず、値がある間は pid が再利用されないので `kill` がほかのプロセスに届かない。
  Rust の `Child` (捨てても待たない) と Haskell の `withCreateProcess` (捨てると止める) は採らなかった。
- シグナルハンドラは書かせず、ブロックして待つ (`waitSignal`) か signalfd でイベントループから受け取る (`signals`・`nextSignal`)。
- `SIGPIPE` はプログラムでは既定のまま (`ignoreSignal sigPipe` で `brokenPipe` にできる)。子には既定に戻し、空のマスクで渡す。
  `output` は書き込みの間だけ `SIGPIPE` をブロックし、入力を読まずに終わった子への残りの入力を捨てる。
- 写像 (`Mapping`) は ω の `Handle` で、読む操作は `IO` (写像中のファイルはほかのプロセスが変えうる)。

**見つけて直した問題 (ランタイムの起動).** ランタイムは `main` を大きなスタックを持つ別のスレッドで動かし、最初のスレッドはそれを
`pthread_join` で待つだけだった。最初のスレッドはシグナルをブロックしていないので、プロセスへ送ったシグナル (`kill -USR2 pid`) が
そちらに届き、プログラムが signalfd で待っていても既定の動作でプロセスが終わった (自己テストが `SIGUSR2` で終了、終了コード 140)。
最初のスレッドはすべてのシグナルをブロックしてから `main` のスレッドを作り、`main` のスレッドは起動時のマスクに戻す。

スレッドプールのスレッドも同じ理由でシグナルを受けてはいけない。はじめは `worker_main` の先頭でブロックしたが、作られてから
ブロックするまでの間に届く競合があり、統合テストで 1 回失敗した (負荷の高いとき)。作る側で非同期のシグナルをブロックしてから
作る (新しいスレッドは作った時点のマスクを継承する) ように直した。プールのスレッドでのブロックを外すと自己テストは 3 回とも
`SIGUSR2` で終わり、戻すと並行に 8 本ずつ 40 回実行して (debug・release のランタイム) すべて通る。

**ヘッダ.** `cirka_rt.h` に、ライブラリが `extern "cirka"` で呼ぶ関数 (v0.0.2 の入出力、FEAT-0241 の `cirka_async_*`・`cirka_sock_*`、
今回の `cirka_os_*`・`cirka_fs_*`) を宣言し、`evloop.rs`・`net.rs`・`os.rs`・`stdio.rs` の公開する関数がすべてヘッダにあることを
単体テスト (`header_declares_the_library_functions`) で調べる。

**サンプルで当たった型推論の問題.** 局所変数で場合分けする `case` を、期待型の分からない位置 (`pure (case r of …)`) に置くと E1200 になる
(BUG-0022)。サンプルは結果を文字列にする補助関数 (`describe`・`okOr`) で書いた。

