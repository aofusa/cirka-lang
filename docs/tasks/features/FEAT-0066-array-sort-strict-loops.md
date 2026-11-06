# FEAT-0066: array-sort の差の調査 (要素の箱・ループの force・自己末尾呼び出しのループ化)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0056, FEAT-0063, FEAT-0065
- 設計: [16-performance.md](../../design/16-performance.md) §4, [07-backend.md](../../design/07-backend.md)

## 概要

`bench/array-sort/own.chs` (100 万要素) が Rust の約 4 倍 (0.37 s / 0.088 s) かかる原因を調べ、取り除く。

調査結果 (`CIRKA_STATS`、`--dump=core-opt`、生成コードの逆アセンブル):

1. **`check` の累積引数のサンクの連鎖** (確保 400 万・max_live 400 万の大半)。`check` は `(acc, ok)` を返すので `acc` について
   正格でなく、`(acc + x * (i rem 7)) rem 1000000007` が要素ごとにサンクになる。`x = a ! i` は境界外で panic しうるので
   投機的に評価することもできない (意味が変わる)。Rust 版は正格なので、ベンチの `own.chs` の `acc` を正格 (`!acc`) にする
   (fannkuch-redux の `own.chs` の `!pc !cs !mf` と同じ扱い)。遅延のままの版は GHC と同じくサンクを作る。
2. **要素の読み出しの箱** (正格にした後も確保 100 万)。`let! x = case a of UArray f -> I# (bytes_get64 f i)` の `case` が
   `let!` の右辺に残り、`x` が既知の構築子にならない。
3. **qsort の内側のループでの配列の force**。ループの引数の配列は呼び出し側がすべて評価済みの値 (newtype の `Unique (UArray f)`)
   を渡すが、MIR はポインタの引数をすべて評価前とみなして毎回タグを検査していた。
4. **自己末尾呼び出しが `musttail` 呼び出し** で、LLVM のループ最適化 (ループ不変式の移動など) の対象にならない。

## 作業内容

- [x] Core の簡約: `let! b = (case s of p -> e) in k` (`s` は変数、選択肢は一つ) を `case s of p -> let! b = e in k` にする
      (`k` の型が分かるとき)。評価の順序は同じで、`e` が構築子なら `b` が既知の構築子になる。また `case` の後の合流点がなくなり、
      フィールドの最後の使用と `k` の確保が同じブロックに並ぶので再利用が効く (`tests/core/run/cirkalang_style.core`: MIR の借用で
      最適化前の確保が 9 → 8 に減ったとき、最適化後も合流点をまたがず 8 になる)
- [x] `ww::whnf_params`: すべての呼び出しが評価済みの値 (評価済みの変数・リテラル・ラムダ・newtype でない構築子、newtype の構築子で包んだ
      評価済みの値) を渡す引数を求める (`evaluated_params` と同じ最大不動点、投機的評価は数えない)。newtype の `case` の
      フィールドも評価済みとする (MIR ではスクルーティニそのもの)
- [x] MIR の下ろし: トップレベルの関数のそのような引数を WHNF とし force しない。newtype の構築子で包んだ変数の `let` は変数の WHNF を引き継ぐ
- [x] LLVM: 自己末尾呼び出し (`tailcall` と `let r = call f; ret r`) を、引数を phi に持つループの先頭 (`tailrec`) への分岐にする
- [x] `bench/array-sort/own.chs` の `check` の累積引数を正格にする
- [x] テスト: Core (`opt_floats_single_alternative_cases_out_of_strict_lets`, `whnf_params_need_values_at_every_call`)、
      LLVM (`self_tail_calls_become_loops`)、実行 (`tests/achc/run/h3/array_sort_unique.hs`: 確保 1000 以下)、MIR の期待出力の更新 (force の除去)

## 受け入れ条件

- [x] array-sort `own.chs` の確保が要素数に比例しない (400 万 → 613)
- [x] qsort の内側のループに force の検査がない (逆アセンブルで確認)
- [x] 他のベンチが遅くならない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

計測 (2026-11-06、ホストに別のビルドの負荷があるため 10 回の最良のユーザー CPU 時間。旧 = 変更前のコンパイラ):

| ベンチ | 旧 | 新 | Rust |
|--------|---:|---:|---:|
| array-sort `own.chs` (遅延の `acc`、変更前のソース) | 0.37 s (確保 4,000,615) | 0.35 s | 0.08 s |
| array-sort `own.chs` (正格な `acc`、1,000,000) | 0.14 s (確保 1,000,615) | **0.11 s** (確保 613) | 0.08 s |
| array-sort `own.chs` (正格な `acc`、4,000,000) | 0.58 s | **0.51 s** | 0.37 s |
| fannkuch-redux `own.chs` / `plain.hs` | 0.05 s / 3.4–3.6 s | 0.03 s / 3.3–3.6 s | |
| n-body `own.chs` / `plain.hs` | 1.83 s / 14.9 s | 1.68 s / 11.4 s | |
| spectral-norm `own.chs` / `plain.hs` | 0.17 s / 0.44 s | 0.17 s / 0.47 s | |
| binary-trees `own.chs` (深さ 18) | 1.27 s | 1.22 s | |

- fannkuch `plain.hs` は最初の計測で 2.96 → 4.64 s と出たが、交互に 3 回測り直すと差はなく (確保・force の回数も同じ)、負荷による揺れ。
- 残る差 (Rust の約 1.4 倍): swap ごとの一意性検査 (`bytes_unique` の RC 比較。FEAT-0067 で除いた)、
  一意性検査の後の境界検査の再計算 (遅い経路のコピーが同じ長さであることを LLVM が知らない)、境界検査そのもの (FEAT-0234)。
- `qsort$w` の入口の force は残る: 複数の結果のポインタのフィールドが評価済みかを追っていないため (ループの外なので影響は小さい)。
