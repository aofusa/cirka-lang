# FEAT-0043: ベンチマーク集と性能レポート

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0016
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

10-testing.md §5 のベンチ (plain.hs / own.chs / main.cirka / Rust) と `bench-report.md`。

## 作業内容

- [x] 各ベンチの実装
- [x] 時間と最大 RSS
- [x] レポート生成

## 受け入れ条件

- [x] 性能目標 (00-overview G3) の達成状況がレポートに記載
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 追加: binary-trees / n-body の `plain.hs`・`own.chs`、spectral-norm (4 実装)、mandelbrot (`plain.hs`・CirkaLang・Rust、点の数を出力)、
  fannkuch-redux (`plain.hs`・Rust)。fasta と文字列処理は CirkaLang の配列 (FEAT-0211) の後。
- 正しさ: `bench/<name>/check` と `tests/bench.rs` (旧 `cirkalang_bench_programs` を置き換え)。
- 最大 RSS: `bench/run.sh` は GNU time (image に追加)、なければ Python の getrusage で測る。
- 結果と G3 の達成状況は [bench-report.md](../../design/bench-report.md) の「ベンチマーク集 (FEAT-0043)」。
  G3 は binary-trees のみ達成 (`own.chs` 0.38、CirkaLang 0.40)。数値計算・配列は 10–40 倍、`plain.hs` は GHC -O2 の 2.8–6 倍。
- 派生チケット: FEAT-0056 (値渡しの引数・局所関数の unboxing)、BUG-0011 (Borrow モナドのコスト)。

