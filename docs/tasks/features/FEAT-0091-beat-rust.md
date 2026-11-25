# FEAT-0091: Rust を上回る性能 (Rust より遅い題材の原因の調査と改善)

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0263
- 設計: [16-performance.md](../../design/16-performance.md), [bench-report.md](../../design/bench-report.md) の「CirkaLang v0.0.7」

## 概要

CirkaLang v0.0.7 のリリースのベンチで、binary-search 1.45・n-body 1.44・cli-grep 1.49・string-split 1.39・session 1.15・
HTTP サーバ 1.29 (16 接続) が Rust より遅く、array-sort・mandelbrot・spectral-norm・stencil・effects は同等 (0.97〜1.01) だった。
性能の方針 (16-performance.md §1: 型・純粋性・一意性・証明から分かる事実を使って Rust を上回る) に従い、題材ごとに差の原因を計測で確かめ、
一般の最適化 (題材ごとの特別扱いをしない) で改善して、すべての題材で Rust を上回ることを目指す。

## 作業内容

- [ ] 題材ごとに差の原因を計測で確かめる (callgrind / cachegrind、生成した LLVM IR とアセンブリの比較、LLVM IR を手で直した実験)
- [ ] 改善を個別のチケットにして実装し、効果を計測して記録する
- [ ] すべての題材を計測し直し、bench-report.md に記録する

## 受け入れ条件

- [ ] すべての題材 (11 題材と HTTP サーバ) で CirkaLang が Rust 以上の速さ (同じ条件で交互に実行した中央値の比が 1.00 以下)
- [ ] 改善の前後で全ベンチの出力が変わらない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- binary-search (1.45): 要素が箱の `Array Int` を読むたびに、要素の `force` (サンクかの検査) と、その前の `dup`・後の `drop` が入る
  (MIR: `array_get_unchecked` → `dup` → `force` → `switch` → `drop`)。内側の反復は約 25 命令 (Rust は約 11 命令。どちらも分岐で、
  予測の外れが多い)。共通の MIR への変換は、要素を遅延のまま格納し、読み出しで `force` する (aCHC の配列の意味論)。CirkaLang の値は
  サンクでない (`Lazy a` も構成子 `Lazy#` の値) ので、要素を評価してから格納すれば、読み出しの `force` は要らず、借用した射影のまま
  `dup` / `drop` も要らない。また、構成子が 1 つの型の `switch` の既定の腕が `panic` なので、LLVM がタグの検査を消せない。
- n-body (1.44): Rust は 5 体の 2 重ループを定数の繰り返し回数で完全に展開する (50 万ステップで分岐 56 万回)。CirkaLang の
  `pairs` / `moves` は (i, j) を持つ末尾再帰のループで、分岐 1,804 万回 (1 ステップ 36 回)・予測の外れ 100 万回 (cachegrind の模擬)、
  命令数は Rust の 1.26 倍 (2.88 億と 2.28 億)。配列の大きさが実行時の値なので、要素の読み出しごとの境界検査も残る。
  定数の引数での呼び出しを特殊化してループを展開すれば、添字が定数になり、検査もループの外に出せる。
- cli-grep (1.49)・string-split (1.39): バイトの検索は共通 Core の 1 バイトずつのループ (`cirka$bytes_find_byte_loop`)。
  LLVM 18 は途中で抜けるループをベクトル化しない。確保は 100 万行で 100 万回 (cli-grep)、200 万回 (string-split)。調査中。
- session (1.15)・HTTP サーバ (1.29): 調査中。
- 2026-11-25 の状況 (FEAT-0097〜0099 の後、i5-3320M、11 回の交互実行の中央値、Rust に対する比): array-sort 1.06・binary-search 1.18・
  cli-grep 1.02・mandelbrot 0.98・n-body 1.16・spectral-norm 1.01・stencil 1.01・string-split 1.06・effects 1.00・session 0.23・
  binary-trees 0.42 (別のプロジェクトのビルドと重なり Rust 側が遅れた。測り直す)。
- n-body・LLVM の版: Rust 1.99.1 は LLVM 23.1、こちらは clang 18 だった。生成した IR を clang-23 でコンパイルし直すと (使い捨ての
  コンテナに apt.llvm.org の clang-23 を入れた実験)、n-body 0.86 倍 (Rust の 0.97)・effects 0.93 倍 (0.95)・stencil 0.98 倍 (0.99)、
  array-sort 1.03 倍 (1.08)・binary-search 1.03 倍 (1.22)・string-split 1.02 倍 (1.07)、他は ±0.5 % (出力はすべて同じ)。
  バックエンドを LLVM 23 に上げる (FEAT-1000、ADR-110)。
