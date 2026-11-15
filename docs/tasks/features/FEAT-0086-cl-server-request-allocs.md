# FEAT-0086: サーバの要求ごとの確保を減らす (継続・Result・文字列のビュー)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0243
- 設計: [16-performance.md](../../design/16-performance.md), [bench-report.md](../../design/bench-report.md) の「CirkaLang v0.0.5」

## 概要

CirkaLang v0.0.5 の HTTP サーバのベンチ (`bench/http-server`) は、並行な接続で Rust の 0.78 倍のスループット (時間で 1.28 倍) だった。
システムコールの数は同じで、差はユーザ空間の命令数 (1 要求あたり約 5,100 と約 700) にあり、その半分以上が確保と解放
(1 要求あたり 15〜20 回) である。要求ごとの確保を除き、Rust と同等のスループットを目指す (第二段階の目標は Rust 以上)。

## 作業内容

- [ ] 1 要求の確保を数えて内訳を記録する (デバッグ用ランタイムの統計か callgrind)
- [ ] 継続を受け取る関数 (`recvOn`・`sendFrom`) を、継続が分かっている呼び出しへ特殊化する (待たずに済んだときに継続のクロージャと
      `Ok d`・`MkTcpStream h` の箱を作らない)
- [ ] `Str` のビュー・`Maybe Int` の結果を、worker/wrapper で箱なしにする (FEAT-0082 と共通)
- [ ] サーバのサンプルの書き方 (連結 `out ++ hello`・残りの複製 `S.fromStr`) を見直し、ライブラリに要るもの (出力のバッファなど) を加える
- [ ] 効果を `bench/http-server/run.sh` で計測しチケットに記録する (16-performance.md §4)

## 受け入れ条件

- [ ] `bench/http-server` の 16・64 接続のスループットが Rust の 0.9 倍以上
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- FEAT-0243 の計測 (callgrind、16 接続 × 500 要求): 確保と解放 (valgrind の下ではランタイムのアロケータが `malloc` に切り替わる) が
  命令の半分以上。ほかは `step` の文字列処理 (9.7%)、バイト列の複製 (6.3%)、`recvOn` (2.4%)。
- 待ちの張り直し (`EPOLLONESHOT` と `EPOLL_CTL_MOD`) は、読んで `EAGAIN` になったときだけ行う。負荷の高いときの回数は perf が使えず
  数えていない (strace の下ではサーバが遅く、ほとんど待たない)。
