# FEAT-0064: fannkuch-redux の一意な配列版と、読み出しの組・累積引数の箱の除去

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0041, FEAT-0123
- 設計: [16-performance.md](../../design/16-performance.md) §4

## 概要

fannkuch-redux は `plain.hs` (リスト) しかなく、Rust (配列) との比が約 100 倍だった。Rust と同じ数え上げのアルゴリズムを
一意な `UArray Int` で書いた `own.chs` を加え、その生成コードに残る確保を Core の最適化で減らす。

- `let! c = (case s of p -> e) in let! y = (case c of …) in k` (`k` は `c` を使わない) を
  `let! y = (let! c = (case s of p -> e) in case c of …) in k` にする。既存の規則 (一つの選択肢の case を外に出す) がその中で効き、
  インライン化した `A.read` の返す組 `(u, Ur x)` を作らなくなる。
- float-in: 遅延 `let` を、別の遅延 `let` を越えて動かす (一意な配列の newtype の包みの後ろにある箱)。
- float-in: 小さく λ を含まない束縛を複数の選択肢が使い、そのどれかが必ず評価する場合、各選択肢に写しを置く
  (束縛子は付け直す)。評価する選択肢では thunk を作らず、その場で計算する。float-in の後に簡約を回す。

## 作業内容

- [x] `bench/fannkuch-redux/own.chs` (Rust と同じアルゴリズム、累積引数は `plain.hs` と同じく正格)
- [x] `let!` の右辺の case を、次の `let!` の右辺に入れる規則
- [x] float-in の拡張 (遅延 `let` を越える、評価される選択肢への写し)
- [x] 計測 (bench-report)

## 受け入れ条件

- [x] `tests/achc/run/h3/fannkuch_unique.hs` (n = 7、`max-allocs: 50000`、`max-array-copies: 0`)。前は約 27 万回
- [x] Core のテスト (`opt_floats_strict_case_into_a_later_scrutinizing_let`、`float_in_passes_lazy_lets_and_copies_forced_bindings`)
- [x] 他のベンチが遅くならない
- [x] テストを追加し `scripts/ci.sh` が通る

## 計測 (2026-11-06、ホスト、3 回の最良値、n = 9)

| 段階 | 時間 | 確保回数 | thunk の評価 |
|------|-----:|-----:|-----:|
| `own.chs` (最初の版) | 0.369 s | 19,774,162 | 725,959 |
| 組を作らない規則 | 0.213 s | 6,164,353 | 725,959 |
| 累積引数を正格に (`plain.hs` と同じ) | 0.162 s | 6,164,351 | 725,959 |
| float-in: 遅延 `let` を越える | 0.138 s | 4,252,846 | 725,959 |
| float-in: 評価される選択肢への写し | **0.099 s** | 3,164,209 | 201 |
| Rust (`rustc -C opt-level=3`) | 0.029 s | | |

- Rust の約 3.4 倍 (`plain.hs` は約 100 倍)。
- 残る確保 (1 順列あたり約 9 回) は `copy` `flips` `next` が返す組 (`Tuple2`/`Tuple3`) と、その中の `Ur`・`I#`。
  再帰関数の組の結果は今の CPR (箱は 1 フィールドの構築子だけ) では消せないので、FEAT-0065 (複数の値の返却) で扱う。
- 他のベンチ (HEAD との比較): array-sort `own.chs` 0.45 → 0.44 s、binary-trees・n-body `plain.hs` は誤差の範囲 (±1〜3%)。
  spectral-norm `plain.hs` は約 1% 遅い (0.443 → 0.448 s)。Core の差は `main` の遅延 `let` の順序だけで、コードの配置の差と見る。
  評価されない写しは作らないことにした (写した `let` が遅延引数になるだけで得がなく、spectral-norm が 4% 遅くなった)。
