# 09. ライブラリ戦略

入力: [docs/specs/02-library.md](../specs/02-library.md)

## 1. 第一段階: aCHC (スコープ縮小版)

### 1.1 前提: GHC のコンパイル済みライブラリは使えない

GHC のライブラリ (`.hi`/`.a`) は GHC の RTS・GC・呼び出し規約に依存しているため、aCHC からリンクできない。
**ライブラリは Haskell ソースから aCHC でコンパイル** する。

### 1.2 `cirka-base` (base 互換)

`lib/cirka-base/` に、GHC の `base` の **公開 API と互換** なライブラリを置く。

```
lib/cirka-base/
  Cirka/Prim.hs          -- プリミティブ (GHC.Prim 相当): Int#, +#, ==#, ... コンパイラ組み込み
  GHC/Types.hs …         -- 必要最小限の GHC.* 互換モジュール
  Prelude.hs
  Data/{List,Char,Maybe,Either,Function,Ord,Tuple,Bits,Word,Int,IORef,Foldable,Traversable,...}.hs
  Control/{Monad,Applicative,Exception}.hs
  System/{IO,Exit,Environment}.hs
  Text/{Read,Show,Printf}.hs
  Numeric.hs
  Foreign/{Ptr,C/Types,C/String,Marshal}.hs
  Cirka/{Ownership,Array,Region,Foreign,Unsafe}.hs   -- aCHC 固有
  Prelude/Linear.hs, Data/Unrestricted/Linear.hs     -- linear-base 互換の最小サブセット
```

- 方針: **GHC の base (BSD-3) のソースを移植できるものは移植**(`Data.List`、`Data.Maybe`、`Text.Show` の多くは純粋な Haskell)。
  GHC 固有の primop・RTS 呼び出しに依存する部分(`GHC.IO`、`GHC.Num.Integer` など)は `Cirka.Prim` とランタイムの上で書き直す。
- 移植元のバージョンとライセンス表記をファイル先頭に残す。
- 優先順位はテストスイートで使われる頻度順(`tests/compat` の失敗理由を集計して決める)。

#### H1 時点の範囲 (FEAT-0105)

型クラスがない H1 では、`Prelude` は **`Int` に単相化した版** を置く (`+`・`==`・`show`・`print` などは `Int` 専用、
リスト・`Maybe`・`Either`・文字列・`IO` の関数は多相)。`Int` だけを使うプログラムは GHC と同じ結果になる。
`Cirka/Prim.hs` は `data Int` などの空のデータ宣言と `foreign import prim "p"` だけからなり、各プリミティブの実体は
脱糖器の Core 定義 `prim$p` (`crates/achc-desugar/src/prims.core`) にある。`Data/Char.hs` は文字の判定と変換の小さな部分集合。
型クラス導入 (H2, FEAT-0110) で `Num`・`Eq`・`Ord`・`Show` などに置き換える。

#### H2 時点 (FEAT-0110)

- `Cirka/Base.hs` が Prelude の実体 (GHC の `GHC.Base` 等に相当)。GHC と同じクラス階層
  (`Eq`/`Ord`/`Enum`/`Bounded`/`Num`/`Real`/`Integral`/`Fractional`/`Floating`/`RealFrac`/`RealFloat`/`Show`/`Read`/
  `Semigroup`/`Monoid`/`Functor`/`Applicative`/`Monad`/`MonadFail`/`Foldable`/`Traversable`) とインスタンス、リスト・文字列・IO の関数を定義する。
  内部の補助関数は `cirka` で始める。
- `Prelude.hs` は GHC の Prelude と同じ名前だけを `Cirka.Base` から再エクスポートする (`toList` などは出さない)。
  `Data.*` モジュール (FEAT-0111) は `Cirka.Base` から必要なものを再エクスポートする。
- `Integer` は FEAT-0114 まで `Int` (64 ビット) の別名、`Float` は `Double` の別名。`Rational` は `Ratio Int`。
- `show :: Double` は GHC と同じ形式 (`0.1`・`1.0e-2`・`1.23456789e7`) をランタイムの `cirka_f64_show_hs` で出す。

### 1.3 数値

| 型 | 実装 |
|----|------|
| `Int`, `Word`, `IntN`, `WordN` | LLVM の整数命令 (オーバーフローは GHC と同じくラップ) |
| `Double`, `Float` | LLVM の浮動小数命令。`sqrt` 等は LLVM 組み込み (`llvm.sqrt.f64`) |
| `sin`, `exp`, `log`, … | **glibc の libm** を FFI で呼ぶ |
| `Integer`, `Natural` | ランタイムの多倍長整数 (Rust で自前実装、小さい値は非ボックス fast path) |
| `Rational` | `Integer` の上に base と同じ Haskell 実装 |
| `showFloat`/`read` | GHC と同じ出力にするため、base の Haskell 実装 (Dragon4 系) を移植 |

GMP は使わない(静的リンク時の LGPL の扱いを避ける・依存を減らす)。性能が問題になれば任意のバックエンドとして追加検討。

### 1.4 Hackage パッケージ

**対象外** (ADR-025)。aCHC の目的は GC なし実行の検証であり、`cirka-base` (縮小版) の範囲で書けるプログラムで十分。
必要になった場合でも、`transformers` 程度の小さく純粋なパッケージをソースから取り込むにとどめる。

## 1A. 第一段階: CirkaLang プロトタイプ (v0.0.x)

| 名前 | 内容 | 版 |
|------|------|----|
| `lib/cirkalang/core` | プリミティブ型・`Maybe`/`Result`/`List`/`Nat`・基本トレイト・`Unique`/`Drop`/`Release`・`Lazy`・証明用の型 | v0.0.1〜 |
| `lib/cirkalang/std` | libc ベース: fs / io / env / process / time (v0.0.2)、thread / net / os (v0.0.5)、math (v0.0.6) | v0.0.2〜 |
| `cirka-linalg` 等 | 数学・線形代数ラッパー (下記 1.5) | v0.0.6 |

方針は第二段階 (§2) と同じで、プロトタイプではその最小部分を作る。

### 1.5 数学・数値計算ラッパー (specs 02、CirkaLang v0.0.6)

specs 02 の「C の成熟した資産を安全な薄いラッパーで使う(再実装しない)」方針は、**CirkaLang v0.0.6** で試作する (FEAT-0252)。aCHC では行わない。以下は API の考え方 (aCHC 風の記法で示す)。

| パッケージ | 内容 | 実装 |
|------------|------|------|
| `cirka-math` | 基本数学関数 | libm の FFI。利用者からは純粋関数に見える |
| `cirka-linalg` | 行列・ベクトル | OpenBLAS / LAPACKE の安全ラッパー |
| `cirka-array` | 高性能配列 | `core` の一意配列 (`Unique (Array a)`) + 行優先/列優先の明示 |

```
-- 所有権を移して出力行列を in-place 更新 (高速)。specs の gemm! に相当
gemmIn : Matrix m k a -> Matrix k n a -> (1 out : Unique (Matrix m n a)) -> Unique (Matrix m n a)
-- 純粋版 (内部で新しい行列を確保)
gemm   : Matrix m k a -> Matrix k n a -> Matrix m n a
```

CirkaLang では次元を依存型で表す (`Matrix m n a`) ので、次元不一致はコンパイル時に検出される。次元は多重度 0 で消去されるので実行時コストはない。

## 2. 第二段階 (CirkaLang v0.1.0〜) の推奨構成

specs 02 の層構成 (core / base / std / 外部パッケージ) を採用する。前提として「まずは言語仕様・言語機能を作り、
標準ライブラリや数学系は glibc を使う」で十分実用になる。

| 層 | 内容 | 依存 | 方針 |
|----|------|------|------|
| `core` | Prelude (map/fold 等の純粋関数)、`Linear` (Owned/Borrow/Drop)、`IO`、`FFI` の基本プリミティブ | コンパイラ・ランタイムと一体 | 純粋で線形性を意識した API |
| `base` | `Data` (Maybe, Either, List, Array, Map, Set)、`Control` (Monad 等)、`Text` (UTF-8)、`System` (ファイル・プロセスを線形資源として) | core | 標準同梱。ほぼ純粋 |
| `std` | `Math`、`Numeric`、`Foreign` (よく使う C ライブラリの安全ラッパー) | base + libc/libm | 標準同梱推奨。内部で C FFI を使う |
| 外部パッケージ | `cirka-linalg` (BLAS/LAPACK)、`cirka-numeric` (GSL 等)、FFT (FFTW)、統計 | std | エコシステムで育てる |

- `core` のうち OS・アロケータに依存しない部分は `core` の **freestanding プロファイル** としてビルドできるようにする(Rust の `core` 相当。組み込み・カーネル向け。必須ではない)。
- リストは純粋・不変、配列は所有/借用を区別して in-place 更新可能、文字列は Rust の `String`/`&str` に近い所有権モデル (所有 `String` と借用 `Str r`)。
- パッケージマネージャは早期に用意する(`cirka.toml`。第一段階の `achc.toml` の依存解決を発展させる)。C ライブラリの安全ラッパーを書くためのテンプレートと `bindgen` を同梱する。

### 推奨と理由

1. **OS とのやりとり (ファイル・ソケット・スレッド・時刻) は glibc を FFI で使う。**
   システムコールの直接呼び出しやアセンブラでの自前実装は、移植性・保守コストに見合わない(Rust・Go 以外の多くの言語も libc を使う)。
   静的バイナリが必要になったら **musl** への切り替えをオプションで用意する(コードは同じ libc API)。
2. **基本的な数学 (`sqrt`, `fma`, `abs`, `floor`, 丸め, ビット演算) は LLVM 組み込み関数で実装する。**
   これは CPU 命令に直接なるため、libm を呼ぶより速く、自前実装の手間もない。アセンブラは不要。
3. **超越関数 (`sin`, `exp`, `log`, …) は当初 glibc libm。** 将来、結果を全プラットフォームで一致させたい場合は、
   MIT ライセンスの **musl libm** や **CORE-MATH** (正しく丸められた実装) を CirkaLang に移植する。純粋な数値計算なので移植は機械的で、良いテストケースにもなる。
4. **SIMD** は LLVM のベクトル型と組み込み関数で提供する(インラインアセンブラは最後の手段)。
5. **線形代数・数値計算は BLAS/LAPACK・GSL・FFTW の安全ラッパー** とし、再実装しない。線形型で「出力バッファの所有権を渡して in-place」版と純粋版の両方を用意する。
6. 数学ライブラリには検証機能 (洗練型・依存型) を積極的に使う(次元・定義域の保証。specs 03 の「数学的な性質が重要なライブラリ」)。

### 第一段階との接続

CirkaLang プロトタイプの `lib/cirkalang/core`・`std` が第二段階の `core`/`base`/`std` の原型になる。aCHC の `cirka-base` は参考 (Haskell の API の取捨選択の材料) にとどめる。
所有権・一意配列の設計結果 (aCHC の `Cirka.Ownership`/`Cirka.Array` と CirkaLang の `core`) は `core.Linear`・`base.Data.Array` に、`cirka-math`/`cirka-linalg` の試作は `std.Math` と外部パッケージに反映する。
