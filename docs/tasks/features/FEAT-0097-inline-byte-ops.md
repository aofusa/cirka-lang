# FEAT-0097: バイト列のコピー・検索・比較をインラインで生成する

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0091, FEAT-0095
- 設計: [07-backend.md](../../design/07-backend.md)、[16-performance.md](../../design/16-performance.md) §4

## 概要

FEAT-0095 でバイト列のコピー・検索・比較を C ライブラリ (`memmove` / `memchr` / `memcmp`) に任せたが、ランタイムの関数
`cirka_bytes_copy` / `cirka_bytes_index` / `cirka_bytes_eq` を経由するため、1 回ごとに呼び出しと範囲の検査 (約 20〜25 命令) がかかり、
長さが定数でも可変長の `memcpy` / `memcmp` を呼ぶ。`bench/cli-grep` の callgrind (n = 20 万) では、Builder へのリテラルの追加
(1 行に 4 回) のコピーが 1 回あたり約 45 命令、行の検索 (`memchr`) が約 20 命令 + `memchr`。さらに `cirka$bytes_write` は範囲を
切り詰める (clamp) ので、リテラルの追加でもコピーの長さが定数にならない。

## 作業内容

- [x] LLVM IR の生成: extern `cirka_bytes_copy` / `cirka_bytes_eq` / `cirka_bytes_index` の呼び出しをインラインに展開する
  (範囲の検査は分岐 1 つにまとめ、失敗は panic。`llvm.memmove` / `memcmp` / `memchr` を直接呼ぶ)。長さが定数なら LLVM が
  ロードとストア (`memmove`)、比較 (`memcmp(..) == 0`) に展開する
- [x] Core の定数畳み込み: 文字列リテラルの `bytes_len` と、範囲内の定数の添字の `bytes_get`
- [x] `cirka$bytes_write` (support.core): 範囲が収まるときは切り詰めずにそのままコピーする (検査は LLVM IR の検査と同じ形にし、
  重複を LLVM が消せるようにする)
- [x] テスト: LLVM IR のスナップショット (インラインの展開)、範囲外のコピー・検索・比較が panic する実行テスト、
  Core の畳み込みの単体テスト
- [x] 全ベンチを計測して記録する

## 受け入れ条件

- [x] cli-grep と string-split が速くなる
- [x] 全ベンチの出力が変わらない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装:
  - LLVM IR (`bytes_op`): `cirka_bytes_copy` / `cirka_bytes_eq` / `cirka_bytes_index` の呼び出しを、長さ 0 以下 (検索は `i >= j`) の判定、
    範囲の検査 (`start < 0 || n > len - start`。コピーと比較は 2 つの範囲を 1 つの分岐にまとめる。失敗は `index out of bounds` の panic)、
    `llvm.memmove` / `memcmp(..) == 0` / `memchr` に展開する。検索はランタイムの関数と同じく、範囲の検査の後にバイトの値 (0〜255) かを調べる。
  - Core (`fold`): 文字列リテラルの `bytes_len` と、範囲内の定数の添字の `bytes_get` / `bytes_get_unchecked`。
  - support.core: `cirka$bytes_write` は、範囲が収まる (`n = j - i` について `0 <= n`, `0 <= i`, `n <= len s - i`, `0 <= off`,
    `n <= len u - off`) ときは切り詰めずに `cirka$bytes_copy_at` でコピーし、それ以外は従来の切り詰め (`cirka$bytes_write_clamped`)。
    検査は LLVM IR の範囲の検査と同じ形なので、インラインのコピーの検査は LLVM が消す。
- 文字列リテラルの長さが定数になると、`toStr` のビューの走査 (`skipSpace` など) が FEAT-0094 で展開され、その複製の連鎖を小さい関数の
  インライン化が 1 回の簡約で指数的に複製して、`tests/cirkalang/run/v002/strings` の Core の最適化が終わらなくなった (BUG-0039 で修正)。
  `tests/cirkalang/run/v006/string_views` も同じ理由で `sumBytes` のループが直線のコードになり、証明で検査を消すテスト
  (`cirkalang_string_views_remove_checks`) の対象の関数がなくなったので、文字列を標準入力から読む形にした。
- レビュー (差分を読むだけのエージェント 3 つと、指摘ごとに反証を試みるエージェント 2 つ) で直したもの:
  - 単体テスト `emits_byte_operations_inline` の panic と `memcmp` の宣言の検査は、宣言がすべてのモジュールにあるので常に真だった。
    `main` の本体で `cirka_panic` の呼び出しを 3 つ (展開ごとに 1 つ) 数える形にした。
  - 範囲外の検索・コピー元の範囲・比較の 2 つめの範囲で panic する実行テストがなかった (`bytes_index_oob` は値 372 で、範囲の検査が
    バイトの値の判定より先であることも確かめる。`bytes_copy_src_oob`、`bytes_eq_second_oob`)。
  - `bytes_inline` の可変長がリテラルから求めていて LLVM が畳み込めたので、標準入力から読む形にした。バイトでない値の検索は -1 だと
    バイトへの切り捨てと区別できないので -140 (切り捨てると `t`) にした。
  - `string_write_clamp` はライブラリの外で `prim` を宣言していたので E1009 で書けなかった。不変条件のない `Builder` で書き先の範囲を
    作る形 (公開の API だけ) に変え、書き元の範囲 (`MkStr` の不変条件で作れない) と `i64` の両端は、support.core の
    `cirka$bytes_write` を 729 通りの (書き先, i, j) でモデルと比べる Rust のテスト (`cirkalang_support_bytes_write_clamps`。
    -O0 と -O2、インタプリタとネイティブ) で確かめる。
  - rustfmt、`unique` の doc コメントの位置。
- 計測 (`docs/artifacts/perf/abtime.sh`。基準 (FEAT-0096)・新・Rust を交互に 15 回 (binary-trees は 5 回) 実行した中央値の比。
  同じホストで別の作業のビルドが走っていて負荷が 1.4〜5 あり、絶対時間は普段の約 2.5 倍なので比だけを見る)。全ベンチの出力は基準と同じ。

  | 題材 | 新/基準 | 新/Rust | FEAT-0096 の Rust との比 |
  |------|--------:|--------:|------------------------:|
  | cli-grep | 0.812 | 1.013 | 1.217 |
  | string-split | 0.834 | 1.044 | 1.186 |
  | array-sort | 1.010 | 1.046 | 1.024 |
  | binary-search | 1.013 | 1.189 | 1.159 |
  | n-body | 1.004 | 1.158 | 1.132 |
  | mandelbrot | 0.996 | 0.986 | 0.938 |
  | spectral-norm | 1.000 | 1.014 | 0.966 |
  | stencil | 0.987 | 1.004 | 0.984 |
  | effects | 0.997 | 1.001 | 0.987 |
  | session | 1.105 (最小値の比 0.959) | 1.282 | 1.120 |
  | binary-trees | 1.105 | 0.432 | 0.386 |

  - session は 2 スレッドの題材で、2 コアのホストに負荷がある間は揺れが大きい (中央値と最小値で向きが逆)。
  - binary-trees の 1.105 はコードの配置による: ホットな関数 (`Main.check$w`・`Main.make$w`) の機械語は基準と同じで、文字列を作る
    関数が大きくなった分アドレスが 16 バイトずれた (0x1700 → 0x16f0) だけ。同じ LLVM IR を関数の先頭を 64 バイトに揃えて
    (`-mllvm -align-all-functions=6`) リンクし直すと基準と同じになる (深さ 19、7 回の最小値: 基準 5.286 秒、新 5.829 秒、新を 64 バイトに
    揃えたもの 5.250 秒)。配置の方針 (関数やループの先頭を揃える) は別のチケットで全題材を計測して決める。

