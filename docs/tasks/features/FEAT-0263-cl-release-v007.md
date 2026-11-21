# FEAT-0263: CirkaLang v0.0.7 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.7 効果・能力・セッション型
- 優先度: P0
- 依存: FEAT-0261
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

効果・能力・セッション型の評価とリリース。

## 作業内容

- [x] リリースのベンチ (`bench/run.sh`、`CIRKA_BENCH_RUNS=3`、`bench/http-server/run.sh`。Docker) と v0.0.6 との交互の比較 — bench-report.md の「CirkaLang v0.0.7」
- [x] 評価メモ — 12-cirkalang.md の「v0.0.7 の評価」
- [x] CHANGELOG (v0.0.7 の節)
- [x] 版を 0.0.7 に上げる (ワークスペース、`cirka` の edition、テストのパッケージ。edition 0.0.6 を受け付けないことをテストする)
- [x] タグ `cirkalang-v0.0.7`

## 受け入れ条件

- [x] 評価メモの記録
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- **非互換 (CHANGELOG に書く)**: 外の資源に名前で触れる std の操作と時刻・乱数が、最初の引数に能力 `Cap k` を取る (FEAT-0260、spec-v0.0.7.md §1.3)。
  v0.0.6 のプログラムは `main` で能力を受け取り、操作に渡すよう書き換える。リポジトリでは std の操作を使うテスト 11 本
  (`fail/net/socket_*`、`run/v002/std_env`・`std_fs`・`tool_wc`、`run/v003/borrow_file`・`do_patterns`、`run/v005/http_echo`・`net_tcp`・`net_udp`・`os_tool`)
  とベンチ 2 本 (`cli-grep`、`http-server`) を書き換えた。
- `main` の型の誤り (E1200) の文言が変わった (能力を受け取った後に `IO ()` であること)。
- リリースのベンチ (2026-11-21、Docker): 2026-11-18 の計測はホストの別の仮想マシンの I/O 待ちで負荷が 5〜13 になったので捨て、負荷が 2 以下のときに
  CirkaLang と Rust を交互に実行して計測し直した (bench-report.md の「CirkaLang v0.0.7」)。G3 (Rust の 1.5 倍以内) を 12 題材すべてで達成
  (binary-search 1.45、n-body 1.44、cli-grep 1.49、string-split 1.39、session 1.15、HTTP サーバ 1.29。他は 0.43〜1.01)。
  v0.0.6 との交互の比較で悪化なし (0.95〜1.05 倍。stencil の 1.05 は時間を使う関数の LLVM IR が同じで、コードの配置による揺らぎ)。
- `scripts/ci.sh` (Docker) はすべて通った。ルートのパーティションの空きが足りないので、ビルドの出力を `docs/artifacts/` の下に置いて実行した。
- Rust より遅い題材の原因の調査と改善は FEAT-0091 で続ける。
