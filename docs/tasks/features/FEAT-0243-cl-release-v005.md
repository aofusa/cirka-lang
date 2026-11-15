# FEAT-0243: CirkaLang v0.0.5 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P0
- 依存: FEAT-0241, FEAT-0242
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

サーバ用途の評価とリリース。

## 作業内容

- [x] HTTP サーバのスループット・レイテンシを Rust と比較 (`bench/http-server`。bench-report.md の「CirkaLang v0.0.5」)
- [x] CHANGELOG
- [ ] タグ `cirkalang-v0.0.5` (タグとリリースの公開は利用者の確認を得てから打つ。AGENTS.md §1)

## 受け入れ条件

- [x] 性能結果と評価メモの記録 (bench-report.md の「CirkaLang v0.0.5」、12-cirkalang.md の「v0.0.5 の評価」)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- ベンチ `bench/http-server`: `server.cirka` (`Std.Async`・`Std.Net`、1 スレッドのイベントループ) と `rust/server.rs` (標準ライブラリだけの
  epoll、1 スレッド) が `GET` に `Hello, World!` を返す。負荷 `load.rs` は接続ごとのスレッドが keep-alive で要求を 1 つずつ送り、
  答えを確かめ、スループットとレイテンシの百分位を出す。`bench/http-server/run.sh` で計測する (`bench/run.sh` は実装のないディレクトリを飛ばす)。
  テスト `http_server` (`tests/bench.rs`) が両方のサーバの答え (パイプライン化・分けて届く要求・ほかのメソッド) を確かめ、負荷を CirkaLang の
  サーバに対して動かす。
- 結果 (Docker、3 回の平均): 1 接続 34.2k / 33.5k 要求/s (CirkaLang / Rust)、16 接続 59.2k / 75.5k、64 接続 57.2k / 73.2k。
  並行な接続で Rust の 0.78 倍 (時間で 1.28 倍)。p99 は CirkaLang が小さい。最大 RSS 2.6 MB / 2.2 MB。
- 差の原因: callgrind で 1 要求あたりの命令数は CirkaLang 約 5,100、Rust 約 700 で、CirkaLang の半分以上が確保と解放 (1 要求あたり 15〜20 回)。
  strace ではシステムコールの数は同じ (受信 1・送信 1)。perf はホストの `perf_event_paranoid` (4) で使えないので callgrind と strace で調べた。
  確保の除去は FEAT-0086 に分けた。
- ほかの題材も計測した (bench-report.md): v0.0.4 から悪化なし。G3 は 9 題材中 7 つ (未達は cli-grep 1.79、string-split 1.59)。
- 版: `Cargo.toml` のワークスペースの版、`cirka` の `EDITION`、パッケージのテストの `edition`、E1004 の文言を 0.0.5 にし、
  0.0.4 のパッケージを受け付けないことをテストに加えた。
- 含まれるチケット: FEAT-0240, 0241, 0242, 0244 (と共通基盤の FEAT-0045, 0046, 0061, 0083, 0084)、BUG-0020〜0024。

