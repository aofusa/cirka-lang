# BUG-0025: ランタイムの `siginfo_t` が 8 バイト短く、`sigtimedwait` がスタックを壊す

- 状態: done
- 重大度: critical
- 発見箇所: FEAT-0047 の作業中、ホストで `cargo test --release -p cirka-runtime` を実行したとき
- 関連: FEAT-0242, FEAT-0047

## 現象

最適化したランタイムの単体テスト `os::tests::signals_waited_for_and_read_from_a_descriptor` が SIGSEGV で落ちる
(`cargo test --release -p cirka-runtime --lib`)。テスト関数から戻った直後、呼び出し側が保存しておいたはずのレジスタ
(`rbx`) が 0 になっており、それを通した書き込みで落ちる。CI はランタイムの単体テストをデバッグビルドでしか実行しないため、
これまで見つからなかった (コミット済みの HEAD でも同じく落ちる)。

## 再現手順

```sh
cargo test --release -p cirka-runtime --lib signals_waited
```

## 期待される動作

C ライブラリが書き込む構造体の大きさは C の定義と一致し、呼び出しの前後でスタックが壊れない。

## 原因

`sys::siginfo_t` の大きさが 120 バイトで、glibc / カーネルの `siginfo_t` (128 バイト) より 8 バイト短かった
(`_rest: [u64; 12]`、正しくは 13)。`sigtimedwait` は 128 バイトを書くので、スタック上の `info` の直後の 8 バイトを 0 で
上書きする。最適化ビルドでは `cirka_os_signal_wait` が呼び出し元にインライン化され、その直後に退避した `rbx` があった。

同じ構造体は `cirka_os_run` (std.os の子プロセスの実行、FEAT-0242) が SIGPIPE を受け取るときにも使っており、利用者の
プログラムでもスタックを壊しうる (メモリ安全性の違反なので critical)。ほかの `sys.rs` の構造体の大きさは glibc と一致していた。

## 修正

- [x] 回帰テスト: `siginfo_t` の直後に番兵を置いて `sigtimedwait` を呼び、番兵が変わらないことを確かめる単体テスト
  (`os::tests::sigtimedwait_stays_within_siginfo`。修正前は番兵が 0 に書き換わって失敗する)
- [x] `siginfo_t` を 128 バイトにする
- [x] `sys.rs` の構造体すべての大きさを glibc (x86-64) の値とコンパイル時に照合する (`const` の `assert!`)
- [x] CI でランタイムの単体テストを最適化ビルドでも実行する (`scripts/ci.sh`、10-testing.md §6)

## メモ

- 修正後、ランタイムの単体テストはデバッグ・最適化ビルドの両方で 47 件すべて通る。
