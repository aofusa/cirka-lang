# 09. ライブラリ戦略

## 1. 第一段階 (aCHC)

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

- `achc.toml` の `[dependencies]` で、ソースディレクトリ(または tarball)を指定してビルド。`.cabal` ファイルの `exposed-modules`・
  `hs-source-dirs`・`default-extensions` だけを読む最小限の Cabal 解釈器を持つ。
- 移植の優先度: `transformers` → `mtl` → `containers` → `array` → `deepseq` → `bytestring` → `text` → `vector` → `random`。
- aCHC で通らない部分はパッチ (`lib/patches/<pkg>-<ver>.patch`) で対応し、パッチの量を互換性の指標として記録する。
- `bytestring`/`text`/`vector` の内部実装は GHC primop 依存が強いため、公開 API 互換の再実装を検討(Unique 配列で in-place 実装できる好例)。

## 2. 第二段階 (CirkaLang) の推奨構成

前提として「まずは言語仕様・言語機能を作り、標準ライブラリや数学系は glibc を使う」で十分実用になる。
そのうえで、Rust の `core`/`alloc`/`std` 分割に倣った **3層構成** を推奨する。

| 層 | 内容 | 依存 | 実装言語 |
|----|------|------|----------|
| `cirka-core` | プリミティブ型、Option/Result、型クラス (Eq/Ord/Num/…)、イテレータ、スライス、文字列処理、基本数学 | なし (OS もアロケータも不要) | CirkaLang + LLVM 組み込み |
| `cirka-alloc` | ヒープを使うデータ構造 (Vec, Map, String) | アロケータ (ランタイム) | CirkaLang |
| `cirka-std` | ファイル・ネットワーク・スレッド・時間・プロセス・環境 | **libc (glibc)** | CirkaLang + FFI |

### 推奨と理由

1. **OS とのやりとり (ファイル・ソケット・スレッド・時刻) は glibc を FFI で使う。**
   システムコールの直接呼び出しやアセンブラでの自前実装は、移植性・保守コストに見合わない(Rust・Go 以外の多くの言語も libc を使う)。
   静的バイナリが必要になったら **musl** への切り替えをオプションで用意する(コードは同じ libc API)。
2. **基本的な数学 (`sqrt`, `fma`, `abs`, `floor`, 丸め, ビット演算) は LLVM 組み込み関数で実装する。**
   これは CPU 命令に直接なるため、libm を呼ぶより速く、自前実装の手間もない。アセンブラは不要。
3. **超越関数 (`sin`, `exp`, `log`, …) は当初 glibc libm。** 将来、結果を全プラットフォームで一致させたい場合は、
   MIT ライセンスの **musl libm** や **CORE-MATH** (正しく丸められた実装) を CirkaLang に移植する。純粋な数値計算なので移植は機械的で、良いテストケースにもなる。
4. **SIMD** は LLVM のベクトル型と組み込み関数で提供する(インラインアセンブラは最後の手段)。
5. **数学ライブラリ(線形代数・乱数・統計)** は `cirka-std` の外に別パッケージとして置き、必要なら BLAS/LAPACK を FFI で使う。

### 第一段階との接続

第一段階で書いた `cirka-base` のうち、純粋な部分(リスト・Maybe・数値の Haskell 実装)は第二段階の `cirka-core` の原型になる。
`Cirka.Ownership`/`Cirka.Array` の設計結果は `cirka-alloc` の API 設計に反映する。
