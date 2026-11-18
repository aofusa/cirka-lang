# FEAT-0253: CirkaLang v0.0.6 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P0
- 依存: FEAT-0251
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

Vulkan compute・FFI 用途の評価とリリース (遠隔 GUI 検証 FEAT-0290 は含めない)。

## 作業内容

- [x] リリースのベンチ (`bench/run.sh`、`CIRKA_BENCH_RUNS=3`、`bench/http-server/run.sh`。Docker) と v0.0.5 との比較 — bench-report.md の「CirkaLang v0.0.6」
- [x] ベンチで見つけた後退の修正: in-place のクイックソートが 2 乗の時間 (BUG-0032)。ベンチの題材が配列を複製しないことをテストで確かめる
- [x] 評価メモ — 12-cirkalang.md の「v0.0.6 の評価」
- [x] CHANGELOG (v0.0.6 の節)
- [x] 版を 0.0.6 に上げる (ワークスペース、`cirka` の edition、テストのパッケージ。edition 0.0.5 を受け付けないことをテストする)
- [x] タグ `cirkalang-v0.0.6`

## 受け入れ条件

- [x] 評価メモの記録
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- ベンチ (2026-11-18、Docker、`CIRKA_BENCH_RUNS=3`) の結果は [bench-report.md](../../design/bench-report.md) の「CirkaLang v0.0.6」、
  評価は [12-cirkalang.md](../../design/12-cirkalang.md) の「v0.0.6 の評価」。最初の計測はホストの他の作業 (ディスクの I/O 待ちで負荷 4〜8) と
  重なったので捨て、負荷が下がってから計測し直した。
- v0.0.5 のコンパイラ (タグ `cirkalang-v0.0.5`) で作ったプログラムと交互に実行した比較は 0.93〜1.02 倍で、悪化した題材はない。
  binary-trees の 1.02 は揺らぎの範囲で、生成した LLVM IR の違いは `main` のループの箱の確保の順序だけ。binary-search と stencil は
  検証に Z3 が要るのでホストでは比べていない (Docker の計測で binary-search は 0.350 s → 0.354 s)。
- 最初の計測で array-sort が 18 時間たっても終わらず、BUG-0032 (in-place のクイックソートが書き込みごとに配列を複製する。
  開発中の 2b3434f から) を見つけて直した。`tests/bench.rs` はベンチの題材が配列を複製すると失敗する。
- 版の更新: ワークスペースの `version`、`cirka` の `EDITION`、構文の E1004 の文言、テストのパッケージの `edition` を 0.0.6 にした。
  edition `"0.0.5"` を受け付けないことを `package.rs` のテストで確かめる。
- v0.0.6 の開発中の版表示は `0.0.5` のままだった (v0.0.5 以前も同じで、AGENTS.md §1 の `v0.0.N-dev` に従っていない)。
  v0.0.7 では最初のチケット (FEAT-0260) で `0.0.7-dev` と spec-v0.0.7.md を作る項目を加えた。
