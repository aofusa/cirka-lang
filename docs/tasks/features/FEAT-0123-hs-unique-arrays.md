# FEAT-0123: 一意配列 (Cirka.Array)

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0121, FEAT-0022
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

`Unique (Array e)` の in-place 操作と freeze/thaw。

## 作業内容

- [x] Array / UArray のランタイム表現 (`Array e` は ARRAY、`UArray e` は 8 バイトの語の BYTES。05-runtime.md)
- [x] new/read/write/length/freeze/thaw (`Cirka.Array`、`Cirka.Array.Unboxed`。ほかに modify/swap/fromListU/(!)/size/fromList/toList/generate/accum)
- [x] in-place 更新の直前の実行時の一意性確認 `array_unique` / `bytes_unique` (Core・MIR・インタプリタ・LLVM・ランタイム) と
      `CIRKA_STATS` の `array_copies`、テストの `-- max-array-copies:` (ADR-040)
- [x] サンクの評価で owned の捕捉変数を dup せずに移動する (ADR-040、04-memory-model.md §4)
- [x] Core 最適化: 評価済みの変数の default だけの `case` の除去、値の引数を持つ構築子の `let!` の既知化、
      1 選択肢の `case` を右辺に持つ `let!` の case への押し込み
- [x] BUG-0009 (タプルの成分の Strict-Linear) を修正

## 受け入れ条件

- [x] 書き込み時に確保が発生しない (`write` のループで確保 0/回、配列のコピー 0。`tests/achc/run/h3/{array,uarray,strict_linear_tuple}.hs` の `-- max-array-copies:`)
- [x] in-place ソートのベンチを記録 (`bench/array-sort`、下記)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

### ベンチ (`bench/run.sh array-sort`、100 万要素の Lomuto クイックソート、2026-11-05)

ホストに他のビルドの負荷があり、絶対値はぶれる (GHC は同じ日に 0.44–0.69 s)。

| 実装 | 時間 | 確保回数 |
|------|-----:|---------:|
| aCHC `Cirka.Array.Unboxed` (`own.chs`) | 1.83 s ± 0.12 | 23.7M |
| aCHC `Cirka.Array` (boxed、参考) | 2.60 s ± 0.40 | 20.0M |
| GHC -O2 `STUArray` (`plain.hs`) | 0.63 s ± 0.05 | — |
| Rust | 0.11 s ± 0.01 | — |

- 配列のコピーはどちらも 0。修正前は同じプログラムで 10 万要素あたり 66,744 回コピーし、確保は 13.4M/10 万要素だった。
- 書き込みの確保は 0/回、一意配列の `read` は Unboxed で 1/回 (要素の箱)。残りの確保の大半は入力生成の LCG の
  遅延した状態と検査の `check` のもの (配列操作ではない)。GHC との差は主にこれと、要素の読み出しの
  `(Unique a, Ur e)` の組の構築 (Unboxed は最適化で消えるが boxed は残る) による。改善は正格性解析側の課題。

### 制限

- `read` は要素を WHNF まで評価する (Core の `array_get` が要素を force するため)。
- 要素型ごとの特殊化はないので、`UElem` の辞書が解決できない多相な文脈では呼び出しが間接になる。
