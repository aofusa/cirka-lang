# FEAT-1010: 浮動小数点の縮約のループを、加算の順序を保ったままベクトル化する

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1000, FEAT-0091
- 設計: [07-backend.md](../../design/07-backend.md) §3, [00-overview.md](../../design/00-overview.md) ADR-115

## 概要

bench/spectral-norm の内側のループ (行と列の積の和) は、1 要素に 1 回の割り算 `divsd` の処理量で律速する
(CirkaLang・Rust とも)。浮動小数点の加算は結合則が成り立たないので、LLVM は x86-64 では和を累積するループを
ベクトル化しない (Rust も同じ)。LLVM の **順序を保つ縮約** (strict / in-order reduction) は、加算だけをレーンの順に
1 つずつ行い、ほかの演算 (割り算・掛け算・変換) を SIMD でまとめる。結果は元のループと 1 ビットも違わず、割り算は
2 つずつ (`divpd`) になる。x86-64 では LLVM の既定で無効なので、`-O2` (LLVM の `-O3`) のときに
`-mllvm -force-ordered-reductions` で有効にする。ベクトル化するかは LLVM の費用のモデルが決める。

実験 (`docs/artifacts/f1009/exp`、15 回の交互実行の中央値、負荷 3.3〜4): 配列にした spectral-norm は Rust の 1.014 倍、
この指定を加えると 0.924 倍 (最小値の比 0.706)。出力は同じ。

## 作業内容

- [x] 設計: 07-backend.md §3、ADR-115
- [x] `cirka-driver`: clang に渡す最適化の引数を 1 つの関数にし、`-O2` で `-mllvm -force-ordered-reductions` を加える
- [x] テスト: 引数の単体テスト、加算の順序で結果が変わる和のループの実行テスト (ネイティブの出力がインタプリタの
  1 つずつの加算と同じ)
- [x] 全ベンチを計測して記録する (bench/spectral-norm は配列の版、FEAT-0091)

## 受け入れ条件

- [x] 縮約のループの結果が変わらない (実行テストでインタプリタと一致)
- [x] spectral-norm が速くなり、全ベンチの出力が変わらず遅くなるベンチがない
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 2026-11-26: FEAT-1010・FEAT-1011 と spectral-norm の配列の版 (FEAT-0091) をまとめた全ベンチ (前 = FEAT-1009 の main の版、
  15 回の交互実行の中央値、binary-trees は 5 回。別の作業の負荷 2.3〜6.1 の中で計測したので比は数 % ぶれる)。出力はすべて同じ。

  | 題材 | 後/前 | 後/Rust |
  |------|------:|--------:|
  | array-sort | 0.929 | 0.950 |
  | binary-search | 0.958 | 0.821 |
  | binary-trees | 0.960 | 0.396 |
  | cli-grep | 1.061 | 0.866 |
  | mandelbrot | 1.001 | 0.990 |
  | n-body | 0.987 | 0.971 |
  | spectral-norm | 0.755 | 0.762 |
  | stencil | 0.955 | 0.959 |
  | string-split | 1.047 | 0.789 |
  | effects | 0.998 | 0.943 |
  | session | 0.851 | 0.252 |

  この指定で生成コードが変わるのは spectral-norm と stencil だけ (ほかのベンチの機械語は同じ)。spectral-norm は Rust の 1.012 倍
  (リストの版) から 0.762 倍になった。cli-grep と string-split の遅れは FEAT-1011 の巨大ページで、`CIRKA_THP=0` では前の版と
  同じ (FEAT-1011 で対象を配列に絞った)。
