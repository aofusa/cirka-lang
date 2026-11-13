# FEAT-0215: CirkaLang v0.0.2 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0211, FEAT-0213, FEAT-0214, FEAT-0216
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

CUI 用途の評価とリリース。

## 作業内容

- [x] サンプル CUI ツール (wc / grep 風) と Rust 版の比較 (`bench/cli-grep`)
- [x] CHANGELOG
- [ ] タグ `cirkalang-v0.0.2` (AGENTS.md に従い、タグとリリースの公開は利用者の確認待ち)

## 受け入れ条件

- [x] 性能結果と評価メモの記録 (`bench-report.md` の「CirkaLang v0.0.2」、`12-cirkalang.md` §8「v0.0.2 の評価」)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `bench/cli-grep`: 100 万行のファイルを `Std.Fs` で書いて読み戻し、行・単語・バイトと `tag7` を含む行を数える。CirkaLang 0.34 s / Rust 0.17 s (2.0 倍)。
  `tests/bench.rs` が引数 1000 で Rust 版と同じ出力・リーク 0 を確認する。
- 全ベンチの結果は bench-report.md に記録 (array-sort・mandelbrot・spectral-norm は Rust と同等、binary-trees 0.49 倍、文字列処理 1.6〜2.0 倍、n-body 10.3 倍)。
- 版: ワークスペースの版と `cirka` の `EDITION` を `0.0.2` にした (`cirka --version` は `CirkaLang v0.0.2`)。テストのパッケージの `edition` も更新。
- タグ `cirkalang-v0.0.2` (と未作成の `cirkalang-v0.0.1`) は、利用者の確認を得てから打つ。

