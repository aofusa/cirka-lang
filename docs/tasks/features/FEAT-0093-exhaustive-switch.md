# FEAT-0093: すべての構成子を覆う switch の既定の腕をなくす

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0091
- 設計: [07-backend.md](../../design/07-backend.md)、[16-performance.md](../../design/16-performance.md) §4

## 概要

MIR の `switch` に既定の腕がないとき、LLVM IR では既定の行き先に `panic` ("no matching switch arm") のブロックを作っていた。
Core → MIR は照合が網羅的でない `case` に明示の `panic` の腕 (`nomatch`) を付けるので、既定の腕のない構成子の `switch` は
型のすべての構成子を覆う。このとき最後の腕を既定の行き先にすれば、構成子が 1 つの型 (`Int` の `I#` など) ではタグの読み出しと
比較が消え、2 つの型では比較が 1 回になる。検査付きのランタイム (`debug_rt`) では、評価されていない値などの不変条件の破れを
見つけるため従来どおり `panic` を残す。

## 作業内容

- [x] LLVM IR の生成: 既定の腕がなく型のすべての構成子を覆う `switch` は、最後の腕を既定の行き先にする (腕が 1 つなら無条件の分岐)
- [x] テスト: LLVM IR の単体テスト (構成子が 1 つの型でタグを読まない、網羅的でない `switch` と検査付きのランタイムは `panic` を残す)
- [x] 全ベンチを計測して記録する (mandelbrot はタグへの `!range` で遅くなった前例がある: FEAT-0058)

## 受け入れ条件

- [x] 構成子が 1 つの型の照合でタグを読まない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 計測 (FEAT-0092 との比較。11 回ずつ交互に実行した中央値の比、負荷 1.5 未満): binary-search 0.923 (0.308 → 0.284 秒。
  `Int` の要素の `I#` の照合でタグを読まなくなった)、binary-trees 0.914、array-sort 0.968、string-split 0.989、cli-grep 0.995、
  spectral-norm 0.999、n-body 1.003、session 1.004、mandelbrot 1.006 (タグへの `!range` で遅くなった FEAT-0058 の現象は出ない)、
  effects 1.009。stencil は 11 回で 1.046 だったが、25 回で計り直すと 0.980 (最小値の比 1.042) で、計測の揺れの範囲
  (同じ日の中でも、同じ Rust 版の時間が 0.175〜0.221 秒と揺れる)。全ベンチの出力は変わらない。
- この時点の Rust との比: binary-search 1.158、n-body 1.472、cli-grep 1.518、string-split 1.409、session 1.119。
