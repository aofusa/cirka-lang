# 性能計測レポート

ベンチマークの結果を記録する (計測方法は [10-testing.md](10-testing.md) §5)。
生データ (hyperfine の Markdown / JSON) は `docs/artifacts/bench/` に出力され、git 管理外。

計測: `scripts/bench.sh <name>` (Docker コンテナ内、`CIRKA_BENCH_RUNS` 回の平均)。

## 計測環境

- CPU: Intel Core i5-3320M (2 コア 4 スレッド, 2.6 GHz)、x86_64 Linux、Docker (Ubuntu 24.04)
- LLVM 18 (clang `-O2`)、rustc (rust-toolchain.toml の版, `-C opt-level=3`)

## M1 ベースライン (FEAT-0016)

### binary-trees (深さ 21, 単一スレッド)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| 手書き MIR + 本ランタイム (`cirka-mir build -O2`) | 15.59 s ± 0.06 | 0.67 |
| Rust (`Option<Box<Node>>`, glibc malloc) | 23.10 s ± 0.01 | 1.00 |

- 受け入れ条件「Rust の 2 倍以内」を満たす (MIR の方が 1.48 倍速い)。
- MIR 版が速い主因はアロケータ: 本ランタイムはサイズクラス別のフリーリスト (05-runtime.md §3) で、
  glibc malloc/free より割り当て・解放が軽い。RC の dup/drop はインライン展開され、
  木の構築 (所有権の移動のみ) では RC 操作がほぼ発生しない。
- 葉の `Nil` は静的オブジェクト (rc=0) を共有するため割り当てない。Rust 版も `None` で割り当てない (公平な比較)。
  最初の Rust 版は `Nil` ごとに `Box` を割り当てており 37.9 s だった。
- 参考: Rust 側を mimalloc 等に替えると差は縮まる見込み。本計測の目的は
  「RC + 本ランタイムが Rust 並みの性能を出せるか」というリスクの早期検証であり、それは確認できた。

## CirkaLang v0.0.1 (FEAT-0207)

計測: `bench/run.sh binary-trees n-body` (Docker コンテナ内、5 回の平均)。CirkaLang は `cirka build -O2`。
出力は Rust 版と一致する (`tests/cirkalang.rs` の `cirkalang_bench_programs` が小さい引数で確認)。

### binary-trees (深さ 21, 単一スレッド)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| 手書き MIR (`cirka-mir build -O2`) | 15.34 s ± 0.11 | 0.68 |
| 手書き MIR + Perceus (`--perceus`) | 15.29 s ± 0.07 | 0.68 |
| **CirkaLang v0.0.1** (`bench/binary-trees/main.cirka`) | 36.29 s ± 0.02 | 1.60 |
| Rust (`Option<Box<Node>>`) | 22.65 s ± 0.15 | 1.00 |

### n-body (5,000,000 ステップ)

| 実装 | 平均時間 | 比 (Rust=1) |
|------|---------:|------:|
| **CirkaLang v0.0.1** (`bench/n-body/main.cirka`) | 20.52 s ± 0.06 | 52.9 |
| Rust (f64 の構造体の配列、SIMD なし) | 0.388 s ± 0.006 | 1.00 |

### 分析

- 主因は **数値のボックス化**。v0.0.1 の `Int` / `Float` は Core で `I# Int#` / `F# Double#` のデータ型であり、
  共通 Core の最適化にまだ worker/wrapper・unboxing がないので、算術の結果ごとに 1 回割り当てる。
  - binary-trees: 木の節 1 つに対して `check` の戻り値の `I#` が 2 つ割り当てられ、`make` の引数の `d - 1` も箱に入る。
    木そのものの構築と解放は手書き MIR と同じ (Perceus・静的な `Leaf`) で、差はほぼ数値の割り当て。
  - n-body: 1 ステップで 10 組 × 約 20 回の浮動小数点演算がすべて `F#` を割り当てる。
    さらに配列と破壊的更新がないので、5 体のリストと `Body` (7 フィールド) を毎ステップ作り直す
    (Perceus の再利用で大半はその場の更新になるが、フィールドの `F#` は新しい箱)。
- 対策 (次版以降の設計に反映):
  - worker/wrapper と unboxing: 正格な `Int`/`Float` 引数・戻り値を `Int#`/`Double#` で渡す (FEAT-0041、v0.0.3 で G3 を目指す)。
    CirkaLang は正格なので、aCHC と違って正格性解析なしで常に unboxing できる。
  - 配列と破壊的更新 (一意な配列、FEAT-0211、v0.0.2) で n-body をRust と同じデータ構造で書けるようにする。
- 結論: GC なしのメモリ管理 (Perceus + 本ランタイム) 自体は手書き MIR で Rust より速いことを M1 で確認済みであり、
  v0.0.1 の差はフロントエンドの出力するボックス化された数値によるもの。v0.0.3 で再計測する。

## Core 最適化 (FEAT-0041)

計測: `bench/run.sh binary-trees n-body` (Docker コンテナ内、`CIRKA_BENCH_RUNS=3`)。CirkaLang は `cirka build -O2`。
変更点は worker/wrapper (正格な箱型の引数・戻り値の unboxing)・正格な箱型フィールドの展開・case-of-case ([06-ir.md](06-ir.md) §1.4)。

### binary-trees (深さ 21)

| 実装 | 平均時間 | 比 (Rust=1) | v0.0.1 |
|------|---------:|------:|------:|
| 手書き MIR (`cirka-mir build -O2`) | 15.32 s ± 0.02 | 0.68 | |
| 手書き MIR + Perceus | 15.36 s ± 0.11 | 0.69 | |
| **CirkaLang** (`main.cirka`) | 12.34 s ± 0.35 | **0.55** | 36.29 s (1.60) |
| Rust | 22.40 s ± 0.05 | 1.00 | |

### n-body (5,000,000 ステップ)

| 実装 | 平均時間 | 比 (Rust=1) | v0.0.1 |
|------|---------:|------:|------:|
| **CirkaLang** (`main.cirka`) | 13.67 s ± 0.01 | **34.7** | 20.52 s (52.9) |
| Rust | 0.394 s ± 0.006 | 1.00 | |

### 確保回数 (`CIRKA_STATS=1`)

| プログラム | 変更前 | 変更後 |
|------|------:|------:|
| binary-trees (深さ 14) | 9,688,585 | 3,222,349 |
| n-body (100,000 ステップ) | 19,300,000 前後 | 13,200,080 |
| aCHC `tests/achc/run/h1/fib.hs` | 88,739 | 728 |
| aCHC `lazy.hs` | 4,131 | 2,199 |
| aCHC `strings.hs` | 1,996 | 1,355 |
| aCHC `map.hs` | 1,204 | 931 |
| aCHC `fact.hs` | 641 | 441 |

### 分析

- binary-trees は数値の箱がほぼ消え、手書き MIR より速くなった (手書き MIR は `check` の結果を毎回箱に入れる書き方のまま)。
- n-body の残りの割り当ては、1 組の相互作用ごとの `Pair` (戻り値の組) とリストのセル。組を返す関数の CPR には複数値の戻り (unboxed タプル) が要るので、
  配列と破壊的更新 (FEAT-0211, v0.0.2) で Rust と同じデータ構造に書き直すのと合わせて再計測する。

## ベンチマーク集 (FEAT-0043)

計測: `scripts/bench.sh` (Docker コンテナ内、`CIRKA_BENCH_RUNS=3`、2026-11-05)。aCHC は `achc build -O2`、CirkaLang は `cirka build -O2`、
GHC は 9.4 の `-O2`、Rust は `rustc -C opt-level=3`。すべての実装が Rust 版と同じ出力を返すことを `tests/bench.rs` が小さい引数で確認する
(各 `bench/<name>/check`)。

今回の計測は以前の節よりマシンの負荷が高く、Rust の binary-trees が 48.1 s (以前は 22–23 s) と遅く出ている。
同じ回の比で比較すること。

### 実装と題材

| 題材 | 引数 | `plain.hs` | `own.chs` | `main.cirka` | Rust | 備考 |
|------|-----:|:-:|:-:|:-:|:-:|------|
| binary-trees | 21 | ✓ | ✓ (木は `LType`、`check` が消費) | ✓ | ✓ | |
| n-body | 5,000,000 | ✓ (正格フィールドのリスト) | ✓ (一意な `UArray Double` を線形に受け渡す) | ✓ (リスト) | ✓ | |
| spectral-norm | 1000 | ✓ (リスト) | ✓ (`UArray Double`) | ✓ (リスト) | ✓ | |
| mandelbrot | 1000 | ✓ | — | ✓ | ✓ | PBM の代わりに集合内の点の数を出力 |
| fannkuch-redux | 9 | ✓ (リスト) | ✓ (一意な `UArray Int`、Rust と同じアルゴリズム、FEAT-0064) | — | ✓ | |
| array-sort | 1,000,000 | GHC のみ (`STUArray`) | ✓ (一意な配列のその場のクイックソート、FEAT-0123) | — | ✓ | |

未実装の題材 (10-testing.md §5): fasta と文字列処理。CirkaLang の配列 (FEAT-0211, v0.0.2) の後に追加する。

### 時間 (平均、比は Rust=1)

| 題材 | aCHC `plain.hs` | aCHC `own.chs` | CirkaLang | GHC -O2 | Rust |
|------|------:|------:|------:|------:|------:|
| binary-trees | 43.94 s (0.91) | 18.42 s (**0.38**) | 19.17 s (**0.40**) | 15.95 s (0.33) | 48.14 s |
| n-body | 25.02 s (38.0) | 6.36 s (9.65) | 26.64 s (40.4) | 7.76 s (11.8) | 0.659 s |
| spectral-norm | 3.51 s (17.5) | 3.01 s (15.0) | 5.06 s (25.2) | 0.655 s (3.26) | 0.201 s |
| mandelbrot | 4.68 s (38.6) | — | 3.51 s (29.0) | 0.894 s (7.37) | 0.121 s |
| fannkuch-redux | 6.04 s (184) | — | — | 1.01 s (30.8) | 0.033 s |
| array-sort | — | 2.14 s (17.2) | — | 0.725 s (5.83) | 0.124 s |

参考: 手書き MIR の binary-trees は 23.63 s (`--perceus` 24.06 s)。

### 最大 RSS

コンテナに GNU time がなかったため、Python の `getrusage` で計測した (fork した Python 自身を含むので約 11 MB が下限。
FEAT-0043 で image に `time` を追加した)。下限を超えたものだけを示す (他はすべて約 11 MB 以下)。

| 題材 | aCHC `plain.hs` | aCHC `own.chs` | CirkaLang | GHC -O2 | Rust |
|------|------:|------:|------:|------:|------:|
| binary-trees | ≤ 11 MB | 194 MB | 194 MB | 13 MB | 258 MB |
| array-sort | — | 216 MB | — | 103 MB | ≤ 11 MB |

- binary-trees: 正格な版 (`own.chs`・CirkaLang・Rust) は長寿命の木 (深さ 21) を実体化するので約 200 MB。遅延評価の `plain.hs` と GHC は
  木の構築と `check` が交互に進み、木全体を持たない。
- array-sort の `own.chs` は要素数に比例するサンク (`CIRKA_STATS`: 10 万要素で forced 100 万、max_live 40 万) を持つ。

### G3 (00-overview) の達成状況

- **`own.chs` / CirkaLang が Rust の 1.5 倍以内**: binary-trees のみ達成 (0.38 / 0.40)。数値計算 (n-body, spectral-norm, mandelbrot) と
  配列 (array-sort) は 10–40 倍で **未達成**。
- **`plain.hs` が GHC -O2 と同等**: **未達成**。GHC 比で binary-trees 2.8 倍、n-body 3.2 倍、spectral-norm 5.4 倍、mandelbrot 5.2 倍、
  fannkuch-redux 6.0 倍。

### 分析 (`CIRKA_STATS=1` の確保回数)

- **箱に入った浮動小数点数** (FEAT-0056): worker/wrapper は正格性解析で正格と分かった引数しか unboxing しない。mandelbrot の
  CirkaLang 版の `go k zr zi cr ci` は `k >= 50` の枝で `zr` などを使わないので箱のまま渡され、反復ごとに `F#` を 2 つ確保する
  (n=200 で 413 万回)。CirkaLang は値渡しなので本来は常に unboxing できる。aCHC の `where` の局所ループは worker/wrapper の対象外で、
  mandelbrot の `plain.hs` は反復ごとにサンクも作る (forced 約 200 万)。
- **配列の要素の箱**: `Cirka.Array.Unboxed` の読み書きは `UElem` クラスのメソッドを通るため、要素が箱に入って返る。
  spectral-norm の `own.chs` は内側の反復 1 回あたり約 3 回確保する (n=300 で 543 万回)。クラスメソッドの特殊化が必要。
  (FEAT-0059 で再調査: `UElem` は BUG-0011 の後に特殊化済み。残りは関数引数 `f` を未知の関数として呼ぶことによる箱で、FEAT-0061 で扱う。)
- **リストによる代用**: CirkaLang v0.0.1 には配列がないので n-body・spectral-norm はリストで書いている (v0.0.2 の FEAT-0211 で書き直す)。
- **遅延評価**: `plain.hs` は局所ループの累積引数・`where` の束縛がサンクになる (GHC は正格性解析と unboxing で除く)。
  fannkuch-redux は要素を強制しないと `take`/`zip` のサンクの連鎖が際限なく伸び、1 GB を使っていた (ベンチ側で強制するよう修正)。
- **借用の抽象化のコスト** (BUG-0011): n-body の `own.chs` を `borrowMut` と `Borrow` モナドで書くと、線形に受け渡す版の約 130 倍遅かった。
  BUG-0011 の修正 (06-ir.md §1.6) の後は約 7 倍 (20 万ステップで 0.91 s 対 0.13 s)。現在の `own.chs` は線形版。
- binary-trees が Rust より速いのは、M1 から変わらず本ランタイムのサイズクラス別フリーリストが glibc malloc より軽いため。

### 次の作業

FEAT-0056 (値渡しの引数・局所関数の unboxing)、BUG-0011、`UElem` の特殊化、CirkaLang v0.0.2 の配列 (FEAT-0211) の後に再計測する。

## 評価済みの引数の unboxing と局所ループの特殊化 (FEAT-0056)

`/usr/bin/time` による 1 回の計測 (アイドル状態のホスト、変更前と変更後のバイナリを交互に実行)。上の節とは計測方法が違うので、比較は同じ表の中で行う。

| 実装 | 引数 | 変更前 | 変更後 | Rust (FEAT-0043 の表) |
|------|-----:|------:|------:|------:|
| CirkaLang mandelbrot | 1000 | 1.99 s | **0.15 s** | 0.12 s |
| CirkaLang spectral-norm | 1000 | 2.58 s | **0.75 s** | 0.20 s |
| CirkaLang n-body | 1,000,000 | 2.90 s | **1.12 s** | (5,000,000 で 0.66 s) |
| aCHC spectral-norm `plain.hs` | 1000 | 1.79 s | **0.45 s** | 0.20 s |
| aCHC array-sort `own.chs` | 1,000,000 | 1.17 s | **0.54 s** | 0.12 s |
| aCHC n-body `own.chs` | 1,000,000 | 0.71 s | **0.45 s** | |
| aCHC mandelbrot `plain.hs` | 1000 | 2.64 s | 2.45 s | 0.12 s |

- CirkaLang の mandelbrot は Rust の約 1.3 倍になり、この題材では G3 (1.5 倍以内) に届いた。内側のループの確保は n=200 で 413 万回 → 5.6 万回。
- aCHC の mandelbrot `plain.hs` は局所ループが持ち上げ・特殊化されるようになったが、`go` の引数が失敗しうる遅延束縛 (`read` した引数) に依存するので
  サンクが残る (Haskell の意味から来る制約)。
- 次回の `scripts/bench.sh` による計測で上の表を更新する。

## LLVM の属性・メタデータ (FEAT-0058)

計測: 同じプログラムの LLVM IR から FEAT-0058 の属性・メタデータを取り除いた版 (前) と付けた版 (後) を clang -O2 でビルドし、
交互に 15 回ずつ実行した最良値 (Docker 外の 4 コアのホスト、2026-11-06)。比は 後/前。

| ベンチ | 前 (s) | 後 (s) | 比 |
|--------|-------:|-------:|---:|
| array-sort `own.chs` | 0.554 | 0.537 | 0.97 |
| binary-trees `main.cirka` | 11.26 | 11.39 | 1.01 |
| binary-trees `own.chs` | 10.90 | 10.43 | 0.96 |
| fannkuch-redux `plain.hs` | 2.89 | 2.97 | 1.03 |
| mandelbrot `main.cirka` (`!range` を除いた最終版) | 0.157 | 0.157 | 1.00 |
| n-body `own.chs` | 2.27 | 2.27 | 1.00 |
| n-body `plain.hs` | 12.02 | 11.50 | 0.96 |
| spectral-norm `own.chs` | 1.44 | 1.42 | 0.99 |
| spectral-norm `plain.hs` | 0.446 | 0.422 | 0.95 |

- 効果は ±5% の範囲で、大きくはない。今の差の主因は確保の呼び出し (`cirka_alloc` は常に関数呼び出し)・箱・サンクで、
  別名の情報で効く場所 (ループの中の load/store の並べ替え・ベクトル化) がまだ少ないためである。
- タグの load に付けた `!range` は mandelbrot を 10% 遅くし (分岐の配置が変わる)、ほかで効かなかったので外した。
  ただし外すと spectral-norm `plain.hs` は約 5% 遅くなる (0.424 s → 0.448 s。FEAT-0061 の計測で判明)。分岐の配置の差で、どちらも小さい。
- 引数の `nonnull`・`noalias`・`readonly` は、MIR に一意性と再利用トークンの区別がないため付けていない (FEAT-0062)。

## 静的な関数引数での特殊化 (FEAT-0061)

再帰の間で変わらない関数引数を、呼び出し側で分かっている関数 (大域関数・自由変数を持つ λ) に置き換えた写しを作る
(`crates/cirka-core/src/sat.rs`)。計測は FEAT-0058 と同じホストで 3 回の最良値 (2026-11-06)。

| ベンチ | 前 | 後 | 確保回数 (n=100) |
|--------|---:|---:|---|
| spectral-norm `own.chs` (n=1000) | 1.44 s | **0.177 s** | 612,842 → 601 |
| Rust (同じホスト、`rustc -C opt-level=3`) | | 0.175 s | |

- spectral-norm `own.chs` は Rust と同等になった (G3 を達成)。`mul f n u` の内側のループと `U.generate n f` が、
  関数引数 `f` を未知の関数として呼んでいたため、反復ごとに `I#` (引数) と `D#` (結果) の箱を作っていた。
- ほかのベンチは変化なし (±4% の範囲。関数引数の写しが作られない)。spectral-norm `plain.hs` では `dot` が呼び出し側でインライン化され、
  `f` は持ち上げた `power$go$w` の中で既に既知 (`f` の代わりに `i` を受け取る) なので、この変換の出番がない。

## 数値ループの生成コード (FEAT-0063)

libm の `sqrt` などを LLVM の組み込み関数にし、`bytes_unique` / `array_unique` の高速経路 (RC == 1) をインラインで出し、
配列の要素に別の TBAA タグを付け、Core で冗長な一意性検査を除く (`crates/cirka-core/src/uniq.rs`)。計測は 3 回の最良値 (2026-11-06)。

| ベンチ | 前 | 後 | Rust |
|--------|---:|---:|---:|
| n-body `own.chs` (5,000,000) | 2.29 s | **0.91 s** | 0.385 s |
| array-sort `own.chs` | 0.54 s | 0.46 s | 0.088 s |

- n-body は `sqrt` が `errno` を書く libm 呼び出しだったため、ループ内の load が呼び出しをまたいで再利用できなかった。
  組み込み関数にすると 1 命令 (`sqrtsd`) になる。
- 一意性検査は n-body で 15 → 8 か所。残りは関数の入口の検査で、引数の一意性を MIR に運ぶ FEAT-0062 で扱う。
- 残る差は境界検査と配列要素のメモリ往復。

## fannkuch-redux の一意な配列版 (FEAT-0064)

`own.chs` を追加し (Rust と同じ数え上げ)、Core の最適化を 2 つ加えた: インライン化した `A.read` の組を次の `let!` の中で分解して作らない規則と、
float-in の拡張 (遅延 `let` を越える、必ず評価する選択肢への写し)。n = 9、3 回の最良値 (2026-11-06)。

| 版 | 時間 | 確保回数 |
|----|-----:|-----:|
| `plain.hs` (リスト) | 2.93 s | |
| `own.chs` (最初) | 0.369 s | 19,774,162 |
| `own.chs` (FEAT-0064 の後) | **0.099 s** | 3,164,209 |
| Rust | 0.029 s | |

- 残りの約 3.4 倍は、再帰関数が返す組 (`copy`/`flips`/`next`) の確保。FEAT-0065 (複数の値の返却) で扱う。

## 複数の値の返却 (FEAT-0065)

結果が組などの単一構築子の型である関数を、複数の値を返す worker にした (06-ir.md §1.7、ADR-049)。3 回の最良値 (2026-11-06)。

| ベンチ | 前 | 後 | Rust |
|--------|---:|---:|---:|
| fannkuch-redux `own.chs` (n = 9) | 0.099 s (確保 3.16M) | **0.055 s** (確保 521) | 0.029 s |
| array-sort `own.chs` | 0.43 s | 0.37 s | 0.088 s |
| n-body `main.cirka` | 5.5 s | 4.5 s | |

- fannkuch-redux は Rust の 1.9 倍 (`plain.hs` のリスト版から数えると約 100 倍 → 1.9 倍)。
- 他のベンチは変化なし (ホストの負荷が高く、binary-trees は深さ 18 で交互に計測: 確保回数は同じで、時間はやや短い)。


## array-sort の差 (FEAT-0066)

`own.chs` が Rust の約 4 倍だった原因は、`check` の遅延の累積引数のサンクの連鎖 (確保 400 万)、要素の読み出しの箱 (100 万)、
qsort のループでの配列の force、自己末尾呼び出しがループにならないこと。累積引数を Rust と同じく正格にし (`!acc`)、
Core の簡約 (`let!` の右辺の単一選択肢の `case` を外に出す)、評価済みで渡される引数の force の省略、自己末尾呼び出しのループ化を加えた。
ホストの負荷が高いため 10 回の最良のユーザー CPU 時間 (2026-11-06)。

| 版 | 時間 | 確保回数 |
|----|-----:|-----:|
| `own.chs` (変更前) | 0.37 s | 4,000,615 |
| `own.chs` (正格な `acc`、変更前のコンパイラ) | 0.14 s | 1,000,615 |
| `own.chs` (FEAT-0066 の後) | **0.11 s** | 613 |
| Rust | 0.08 s | |

- 残る差 (約 1.4 倍) の一意性検査は FEAT-0067 で除いた。内訳は次節を参照。他のベンチは揺れの範囲で変化なし。

## ループの入口の一意性検査の除去 (FEAT-0067)

配列の引数を一意性検査してから自己末尾呼び出しに渡すループに、引数が一意だと仮定した関数 (`f$u`) を加え、
検査の済んだ後の反復の一意性検査をなくした (MIR、`crates/cirka-mir/src/uniq.rs`)。
ホストの負荷が低いときに、前後と Rust を交互に実行した。10 回の最良のユーザー CPU 時間の 3 組の最良 (2026-11-06)。

| ベンチ | 前 | 後 | Rust |
|--------|---:|---:|---:|
| array-sort `own.chs` (1,000,000) | 0.11 s | **0.10 s** | 0.08 s |
| array-sort `own.chs` (4,000,000) | 0.48 s | **0.44 s** | 0.35 s |
| n-body `own.chs` (5,000,000) | 0.89 s | **0.70 s** | 0.38 s |
| fannkuch-redux `own.chs` (10) | 0.44 s | **0.40 s** | 0.22 s |
| spectral-norm `own.chs` (1000) | 0.17 s | 0.17 s | 0.17 s |

- partition のループでは、一意性検査の分岐だけでなく、配列のポインタと長さがループ不変になることも効く。
  最適化後の IR で、境界検査の失敗ブロックは 8 → 4 か所、ポインタの phi は 4 → 0 個になる。
- array-sort の差の内訳 (400 万要素): 生成 IR の partition のループから手で検査を除くと、一意性検査なしが 0.44 s (このパスの結果と同じ)、
  さらに残る 4 か所の境界検査もなしが 0.40 s。Rust との差 0.09 s のうち境界検査 (FEAT-0234) は 0.04 s で、残りの 0.05 s は未調査。
