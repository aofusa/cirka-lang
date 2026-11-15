# 08. C FFI

`docs/specs/01-cffi.md` の構想(仮称 LinHask)を具体化したもの。
目標は「宣言は Rust/Swift/cgo 並みに簡単、安全性は線形型で保証、オーバーヘッドはゼロ」。

ABI の実装 (構造体・共用体・可変長引数・コールバック: FEAT-0047) と bindgen (FEAT-0048) は **共通基盤** で、
aCHC と CirkaLang は宣言の構文だけが異なる。本書の §1〜§7 は aCHC の記法で書き、CirkaLang の記法は §8 に示す。
実用目標 (サーバ・syscall・Vulkan compute) に必要な要件は §9。

## 1. 三つの水準

| 水準 | 方法 | モード | 安全性 |
|------|------|--------|--------|
| Haskell 2010 互換 | `foreign import ccall "math.h sin" c_sin :: CDouble -> CDouble` | `.hs` / `.chs` | 生ポインタ `Ptr a`(GHC と同じ) |
| 標準 | `extern "C" { ... }` ブロック + `OwnedPtr`/`BorrowedPtr`/`Nullable` | `.chs` | 所有権を型で保証 |
| 自動 | `import "C" "header.h" (names)` / `achc bindgen` | `.chs` | 既定の推測 + 利用者が上書き |

## 2. 型の対応

| C | aCHC | 種 |
|---|------|----|
| `int`, `long`, `size_t`, `double`, … | `CInt`, `CLong`, `CSize`, `CDouble`, … (`Foreign.C.Types`) | Type |
| `T*` (所有権なし・生) | `Ptr T` | Type |
| `T*` (呼び出し側が解放責任を持つ) | `OwnedPtr T` | **LType** |
| `const T*` / `T*` (一時的に貸す) | `BorrowedPtr r T` | Type (`withBorrowed` のスコープ内でのみ有効) |
| NULL になりうる | `Nullable p` (= `Maybe p` と同表現: NULL ⇔ `Nothing`) | p に従う |
| `char*` (NUL 終端) | `CString` = `Ptr CChar` (GHC 互換・生)、所有なら `OwnedCString` (LType) | |
| 関数ポインタ | `FunPtr (a -> b)` | Type |
| 構造体 | `extern struct` 宣言 (レイアウトを C と一致) | 指定 |

## 3. `extern` ブロック

```haskell
{-# LANGUAGE CirkaExtern #-}
module File where

extern "C" link "c" {
  type CFile                                               -- 不透明型

  fopen  :: CString -> CString -> IO (Nullable (OwnedPtr CFile))
  fclose :: OwnedPtr CFile %1 -> IO CInt
  fgets  :: Ptr CChar -> CInt -> BorrowedPtr CFile -> IO (Nullable (Ptr CChar))
  strlen :: CString -> CSize                                -- IO なし = 純粋と宣言
  printf :: CString -> CVarArgs -> IO CInt                  -- 可変長引数

  struct Timespec { tv_sec :: CLong, tv_nsec :: CLong }     -- C と同じレイアウト
  clock_gettime :: CInt -> MutPtr Timespec -> IO CInt
}

-- 安全なラッパ (利用者が書く)
type File :: LType
newtype File = File (OwnedPtr CFile)

instance Release File where
  release (File p) = () <$ fclose p

openFile :: String -> String -> IO (Maybe File)
openFile path mode =
  withCString path \cp -> withCString mode \cm ->
    fmap File . toMaybe <$> fopen cp cm
```

- 戻り値に `IO` がない宣言は **純粋** とみなす(宣言者の責任。`strlen` 等)。
- `extern` 内の `BorrowedPtr T` には、コンパイラがスコープ変数 `r` を補い、結果型を借用モナドに持ち上げる
  (§4)。例: `fgets` の実際の型は `Ptr CChar -> CInt -> BorrowedPtr r CFile -> BorrowIO r (Nullable (Ptr CChar))`。
- `OwnedPtr T %1` を受け取る関数は所有権を C 側へ移す(以後 aCHC 側は解放しない)。
- `OwnedPtr T` には既定の `Drop` も `Release` もない(どの関数で解放すべきか分からないため)。必ずラッパ型で `Drop`/`Release` を定義するか、
  `extern` 宣言に `free with fclose` 属性を付けると `Release` が自動生成される:
  `fopen :: CString -> CString -> IO (Nullable (OwnedPtr CFile)) free with fclose`

## 4. 借用ポインタ

```haskell
type BorrowIO :: Region -> Type -> Type    -- IO を行える借用モナド (liftIO :: IO a -> BorrowIO r a)
withBorrowed    :: OwnedPtr a %1 -> (forall r. BorrowedPtr r a -> Borrow r b)   %1 -> (OwnedPtr a, b)
withBorrowedIO  :: OwnedPtr a %1 -> (forall r. BorrowedPtr r a -> BorrowIO r b) %1 -> IO (OwnedPtr a, b)
withCString     :: String -> (CString -> IO b) -> IO b    -- GHC 互換 (生ポインタ。脱出は検査しない)
```

`BorrowedPtr r a` は [02 §5](02-language-extensions.md) の借用と同じく、ポインタを使う計算の型に必ず `r` が現れるように
`Borrow r` / `BorrowIO r` に閉じ込めて脱出を防ぐ(純粋関数や `IO` のクロージャにすると `r` を含まない型で持ち出せてしまうため)。
C 側がポインタを保存してしまう場合の安全性は保証できない(宣言者の責任)。
GHC 互換の `Ptr`/`CString` は従来どおり生ポインタで、安全性は Haskell と同程度(`unsafe` 扱いではないが検査もしない)。

## 5. コールバック

```haskell
mkCallback :: (CInt -> IO CInt) -> IO (Callback (CInt -> IO CInt))   -- Callback :: LType (Release で解放)
callbackPtr :: Ref r (Callback f) -> Borrow r (FunPtr f)   -- C 側が保持してよいのは Callback が生きている間のみ
```

クロージャを保持するトランポリンを確保し、`Release` で解放する。

## 6. bindgen と `import "C"`

共通の `cirka-bindgen` (FEAT-0048) が C ヘッダを libclang で解析し、言語に依らない宣言の表 (関数・構造体・共用体・列挙・型の別名・定数) を作って、
言語ごとのプリンタで出力する。

```
achc bindgen [options] header.h [-o Math.hs] [-- clang の引数...]
cirka bindgen [options] header.h [-o math.cirka] [-- clang の引数...]
  --allowlist=PATTERNS   出力する宣言の名前 (`|` 区切りのグロブ、`*` は任意の列)。参照する型は自動で加わる
  --file=PATTERNS        出力する宣言を書いたファイルのパス (グロブ)
  --pure=PATTERNS        IO なしで宣言する関数 (`sin` など。純粋かどうかは宣言者の責任)
  --link=LIB             リンクするライブラリ (CirkaLang は `link "LIB"`、aCHC は注釈)
  --module=NAME          モジュール名 (aCHC の既定は出力ファイル名、CirkaLang は既定で書かない)
  -I DIR  -D NAME[=V]  -U NAME   clang へ渡す
```

**libclang。** コンパイラの実行ファイルは libclang をリンクせず、`bindgen` が実行時に `dlopen` で読み込む
(`CIRKA_LIBCLANG`。空なら `libclang-<CIRKA_LLVM_VERSION>.so.1` などを探す)。clang の組み込みヘッダ (`stddef.h` など) は
`CIRKA_CLANG -print-resource-dir` の場所を使う。ヘッダのエラーは clang の診断 (ファイル:行:桁) で報告し、何も出力しない。

**選ぶ宣言。** 既定ではヘッダが取り込むすべての宣言を出す (Rust の bindgen と同じ)。予約された名前 (`__x`、`_X`) と
clang の組み込みヘッダの宣言は出さず、予約された名前の `typedef` は透過して元の型で書く。`--allowlist`・`--file` を与えると、
両方に合う宣言を根として、根から型で参照される宣言だけを出す (列挙はその名前か定数の名前が合えば根)。
`--file` のパターンは宣言を書いたファイルの絶対パス (`.` と `..` を除いたもの) と照合する (`--file='*/vulkan_core.h'`)。
`static` な関数 (インライン) と大域変数は出さない (注釈を出す)。

**型。**

| C | aCHC | CirkaLang |
|---|------|-----------|
| `char` `signed char` `unsigned char` | `CChar` `CSChar` `CUChar` | `CChar` `CChar` `CUChar` |
| `short` `int` `long` `long long` (と `unsigned`) | `CShort` `CInt` `CLong` `CLLong` (`CU…`) | `CShort` `CInt` `CLong` `CLong` (`CU…`) |
| `int8_t` 〜 `uint64_t` | `Int8` 〜 `Word64` | 大きさと符号が同じ C の型 (`CChar` `CUChar` `CShort` … `CULong`) |
| `size_t` `ssize_t` `intptr_t` `uintptr_t` `ptrdiff_t` `wchar_t` | `CSize` `CLong` `CIntPtr` `CUIntPtr` `CPtrdiff` `CWchar` | `CSize` `CSSize` `CLong` `CULong` `CLong` `CInt` |
| `_Bool` / `bool` | `CBool` | `CBool` |
| `float` `double` | `CFloat` `CDouble` | `CFloat` `CDouble` |
| `T *` (`const T *` も) / `void *` | `Ptr T` / `Ptr ()` | `Ptr T` / `Ptr ()` |
| 関数ポインタ | `FunPtr (A -> IO R)` | `FunPtr (A -> IO R)` |
| フィールドの配列 `T[n]` | `[T]` (多次元は平らにする) | `CArray n T` |
| `long double`・`__int128`・`_Complex`・ベクトル型 | 扱わない (その宣言を飛ばして注釈を出す) | 同左 |

- **所有権と借用は推測しない** (ポインタはすべて `Ptr`)。生成したファイルを利用者が編集するか、安全な型で包む
  (Rust の bindgen と `-sys` クレートの分け方)。`const T*` を `BorrowedPtr` にする当初の案はやめた (借用元の所有者がない値には使えない。ADR-093)。
- 関数の結果には `IO` を付ける (`--pure` に合う関数を除く)。

**構造体と共用体。** フィールドの位置・大きさ・境界を clang の値と `cirka_core::ctype` のレイアウト計算 (extern ブロックの構造体が使う、§7.2) で比べて確かめる。
一致しないもの (`alignas`、格納単位にまとめられないビットフィールド、値として表せない型のフィールド) は **不透明型** として出し、理由を注釈に書く。
- ビットフィールドは格納単位 (宣言の型の大きさに揃った単位) ごとに 1 つの符号なし整数のフィールド `bitfieldN` にまとめ、各ビットフィールドの位置と幅を注釈に書く。
- 名前のない構造体・共用体の型は `<親>_<フィールド>` (名前のないメンバは `<親>_anonN`) と名付け、名前のないメンバ (C11 の無名メンバ) は `anonN` というフィールドにする。
- フレキシブル配列メンバ (`char name[];`) は大きさに含まれないので省き、注釈に書く。前方宣言だけの構造体 (`struct VkInstance_T`) は不透明型。

**定数。** オブジェクト形式のマクロは、ヘッダを取り込んだ 2 つ目の翻訳単位で `__auto_type v = MACRO;` と宣言して **clang に評価させる**
(`VK_MAKE_API_VERSION(...)` のように関数形式のマクロを使うものも値になる)。整数・浮動小数・文字列になったものを推論された型の定数として出し、
式にならないもの・ポインタ・有限でない浮動小数は出さない。`static const` の整数の変数 (Vulkan の 64 ビットのフラグ) と列挙定数も定数にする。

**aCHC のプリンタ。** Haskell 2010 の FFI と `Storable` で書くので、`.hs` (GHC でも) と `.chs` のどちらでも使える。
extern ブロックで書く出力は FEAT-0130 で加える。
- 関数は `foreign import ccall "sym" c_sym :: …`。値の名前は C の名前に `c_` を付ける (Haskell の予約語や Prelude と衝突せず、大文字で始まる C の名前も使える)。
  構造体の値渡しと可変長引数は Haskell 2010 の FFI で書けないので飛ばす (注釈)。関数ポインタの `typedef` には `dyn_T` (`dynamic`) と `wrap_T` (`wrapper`) を出す。
- 構造体は同名の構成子のレコード (フィールドは `<型名の先頭を小文字にしたもの>_<フィールド>`) と、clang のオフセットで読み書きする `Storable` のインスタンス。
  共用体は大きさ分のバイト列の newtype (`newtype U = U [Word8]`)。
- 列挙は `newtype T = T CInt` (列挙の整数型) と定数 `c_NAME :: T` と `Storable`。名前のない列挙の定数とマクロは C の型の定数。型の名前は先頭を大文字にする (`timespec` → `Timespec`)。

**CirkaLang のプリンタ。** §8 の extern ブロックの宣言 (`type`・`enum`・`const`・`struct`・`union`) で書く。構文と意味は FEAT-0250 で実装する。
型・定数の名前は大文字で始め、フィールドと引数の名前が予約語なら後ろに `_` を付ける。大文字で始まる C の関数は宣言できないので飛ばす (注釈)。

`import "C" "math.h" (sin, cos)` (ビルド時に bindgen を走らせて取り込む糖衣) は extern ブロックの実装 (FEAT-0130 / FEAT-0250) の後に加える。

## 7. 実装

- 型検査: `extern` 宣言は通常の型シグネチャとして扱い、C ABI で表現可能な型か(`CInt` 等・ポインタ・構造体のみ)を検査する。
- MIR: `extern f(args)` 命令。LLVM では `declare` + `call ccc`。構造体の値渡し・可変長引数は x86_64 SysV ABI に従い展開する。
- `IO` 付きの呼び出しは `State#` トークンで順序付けられ、最適化で並べ替えられない。

### 7.1 Haskell 2010 水準の実装 (FEAT-0116)

- 共通 Core / MIR の `extern` 宣言に **C シグネチャ** (`(c (i32 ptr) i32)` / `as c(i32, ptr) -> i32`) を付けられる。
  C の型は `i8 u8 i16 u16 i32 u32 i64 u64 f32 f64 ptr void` (x86_64 SysV のスカラー)。Core 側の値は整数とポインタが `Int#`、
  浮動小数が `Double#`、`void` が `Unit#` のままで、LLVM バックエンドが呼び出しの前後で変換する
  (`trunc` / `inttoptr` / `fptrunc`、結果は `sext` / `zext` / `ptrtoint` / `fpext`)。`i8`/`i16` の引数には `signext`/`zeroext` を付ける。
  C シグネチャのない `extern` はランタイムの規約 (`Int#` = `int64_t` …) のまま。
- 脱糖器は `foreign import ccall "[static] [h.h] name" f :: T` (`capi` も同じ扱い、`safe`/`unsafe` は無視) を、C シグネチャ付きの
  `extern` と、引数の箱を開けて結果を箱に入れる Core のラッパに変換する。結果が `IO t` なら `Unit# ->` のラムダ (IO の表現) にする。
  型は newtype を剥がして `Int`/`Double`/`Char` に達するまで辿り、途中で最初に現れた既知の型 (`CInt`→`i32`、`Word8`→`u8`、
  `Float`→`f32`、`Ptr`→`ptr` …) で C の型を決める (利用者の newtype も渡せる)。同じ C 関数を異なる型で取り込むと E0618。
  アドレスの取り込み (`&sym`)・`dynamic`・`wrapper` (コールバック) は §7.2 (FEAT-0047) で加えた。
- 値: `Ptr a` / `FunPtr a` はアドレスの `Int` の newtype。`CInt` などは `Int32` などの newtype で、`Int8`〜`Word32` は
  符号拡張 / ゼロ拡張した `Int` の newtype (演算ごとに正規化)、`Word64` は `Word` の newtype。正規化してあるので
  64 ビットのレジスタで渡しても ABI 上正しい。
- メモリ: `peek`/`poke` はランタイムの `cirka_mem_read` / `cirka_mem_write` (`_f` は浮動小数)。`malloc`/`free`/`memcpy` などは
  ライブラリ自身が `foreign import ccall` で呼ぶ。C ヒープのメモリは RC の対象外 (Rust の生ポインタと同じく利用者の責任)。
- MIR インタプリタ (テストの基準) は `dlsym` で関数を探し、整数 6 個・浮動小数 8 個の引数を持つ固定の関数型で呼ぶ
  (SysV は二つのレジスタクラスを独立に順に割り当て、余分な引数は無視されるため)。`float` は XMM の下位 32 ビット。
  スタック渡しの引数・可変長引数・構造体は §7.2 で加えた。これが `cirka-mir` で唯一の `unsafe` (`ffi.rs`)。

### 7.2 ABI の完成 (FEAT-0047)

共通 Core / MIR の C シグネチャに、構造体・共用体の値渡し、可変長引数、関数ポインタの呼び出し、シンボルのアドレス、
コールバックを加える。フロントエンドの記法 (CirkaLang の `extern struct` など) は FEAT-0250、ヘッダからの生成は FEAT-0048。

**C の集成型。** プログラムに C の構造体・共用体の表を持つ。フィールドは名前を持たず、型の並びだけを持つ
(フィールドの位置はフロントエンドが `cirka_core::ctype` のレイアウト計算で求める)。

```
(cstruct timespec (i64 i64))
(cstruct vec3 (f32 f32 f32))
(cunion clear_value ((array f32 4) (array i32 4)))
(cstruct header :packed (u8 u32))          ; __attribute__((packed))
(cstruct rect (offset2d extent2d))          ; 入れ子は名前で参照
```

- フィールドの型はスカラー (`i8` … `ptr`)、集成型の名前、配列 `(array T n)` (n ≥ 1)。空の構造体・値による自己参照は Lint のエラー。
- レイアウトは x86_64 SysV (= gcc / clang): フィールドを自分の境界に揃えて並べ、大きさを全体の境界の倍数に切り上げる。
  共用体は全フィールドが位置 0 で、大きさは最大のフィールドを境界に切り上げたもの。`:packed` は境界を 1 にする。
  ビットフィールドと `alignas` は扱わない (bindgen が格納単位の整数として出す、FEAT-0048)。

**集成型の値の受け渡し。** Core の値は整数・浮動小数・オブジェクトのポインタだけなので、集成型は **メモリ上のアドレス (`Int#`)** で表す。

- 引数が集成型なら、Core の引数はその値が置かれたアドレス。バックエンドが ABI に従ってメモリから読んで渡す。
- 結果が集成型なら、Core の引数の先頭に **結果を書き込むアドレス** を 1 つ足し、Core の結果は `Unit#` にする。
  メモリはフロントエンドが用意する (境界に揃えた大きさ分)。
- 呼び出しの分類は SysV の規則: 16 バイト以下で境界に揃ったフィールドだけなら 8 バイト単位 (eightbyte) ごとに
  INTEGER (整数・ポインタを含む) / SSE (浮動小数だけ) に分け、整数レジスタ / XMM レジスタで渡す。
  16 バイトを超えるか揃っていないフィールドがあれば MEMORY (引数はスタックへの複写 `byval`、結果は呼び出し側の領域 `sret`)。
  レジスタが足りない集成型の引数は、clang と同じく全体をスタックで渡す (`sret` は整数レジスタを 1 つ使う)。
- LLVM IR では clang と同じく eightbyte ごとの型に分ける: INTEGER は残りの大きさの `iN` (`i64`、末尾は `i32`・`i24` など)、
  SSE は `double` / `float` / `<2 x float>`。結果が 2 つなら `{T0, T1}` を返す。

**可変長引数。** C シグネチャの `...` 以降が可変部 (`(c (ptr ... i32 f64) i32)`、MIR では `as c(ptr, ..., i32, f64) -> i32`)。

- 可変部の型は既定の実引数昇格の後の型に限る (`f32`・`i8`・`u8`・`i16`・`u16` は Lint のエラー。フロントエンドが `f64` / `i32` に昇格する)。
- 呼び出しごとに可変部の型が違うので、Core の `extern` に **リンク名** (`:link printf`、MIR では `link printf`) を付けて、
  同じ C の関数を別々の Core の名前で何度でも宣言できる。同じリンク名の宣言は固定部と結果が一致しなければならず、
  可変長でない関数は C シグネチャ全体が一致しなければならない (LLVM の宣言は 1 つ)。
- LLVM は可変長の呼び出しで `al` (使った XMM レジスタの数) を設定する。

**extern の種類。** C シグネチャを持つ `extern` に種類を付ける (既定は名前で呼ぶ関数)。

| 種類 | Core | Core の型 | 意味 |
|------|------|-----------|------|
| 呼び出し | `(extern f (…) r (c …))` | C の引数の表現 | C の関数 (リンク名) を呼ぶ |
| `:dynamic` | `(extern f (Int# …) r (c …) :dynamic)` | 先頭が関数のアドレス | 関数ポインタを呼ぶ (Haskell の `dynamic`、`vkGetInstanceProcAddr` の結果) |
| `:addr` | `(extern f () Int# [(c …)] :addr :link sym)` | 引数なし、`Int#` | シンボルのアドレス (`&sym`)。C シグネチャがあれば関数、なければデータ |
| `:callback h` | `(extern f (T) Int# (c …) :callback h)` | 環境 1 つ、`Int#` | 呼ぶと `h` を呼ぶ C の関数ポインタを新しく作る |

**コールバック。** C の関数ポインタはコードのアドレスなので、クロージャ (環境) を持たせるには呼ばれたときに環境を見つける仕組みが要る。
実行時にコードを書き込むと W^X に反するので、**ランタイムに固定個のスタブを予め置く** (libffi の静的トランポリンと同じ考え方)。

- ランタイム (`callback.rs`) が `.text` に 4096 個のスタブを置く。スタブは 16 バイトで、自分のアドレスを `r10` に入れて共通の入口へ跳ぶ。
  入口はスタブの番号から表の項目 `{環境, 行き先}` を求め、**環境を `r10` に入れて行き先へ跳ぶ**。引数のレジスタとスタックには触れない。
  コールバックを使わないプログラムでは、リンカがスタブと表を取り除く (`--gc-sections`)。
- 行き先は `:callback h` ごとにバックエンドが作る C の呼び出し規約の関数 (トランポリン) で、LLVM の `nest` 属性 (x86_64 では `r10`) で環境を受け取る。
  C の引数を Core の表現に直し (集成型はスタック上の領域のアドレス)、`h 環境 引数…` を呼び、結果を C の型に戻して返す。
- `h` はトップレベルの関数で、型は `T -> [Int# (集成型の結果の書き込み先)] -> 引数の表現… -> 結果の表現`、引数の数だけのラムダで書く。
  環境 `T` は箱に入った値。トランポリンは呼ぶたびに環境を dup する (`h` が所有して受け取る場合)。
- `:callback` の呼び出しは `cirka_callback_new(トランポリン, 環境)` で表の空きを取り、スタブのアドレスを返す。環境の所有はスタブに移る。
  C はどのスレッドからでも呼びうるので、環境とそこから届くオブジェクトは共有にする (`cirka_mark_shared`、参照カウントが原子的になる。05-runtime.md §9.3)。
  `cirka_callback_free(アドレス)` が表を空け、環境を drop する (Haskell の `freeHaskellFunPtr`)。解放した後に C から呼ぶのは利用者の誤り。
  4096 個を使い切ると終了コード 251 で止まる。
- 制約: コールバックはランタイムが知っているスレッド (最初のスレッドとランタイムが作ったスレッド) から呼ばれること、
  `h` の中の panic はプロセスを終える (C のフレームを巻き戻さない、FEAT-0040)、可変長のコールバックは作れない。

**インタプリタ。** 固定の関数型を C の可変長関数の型 (`fn(i64, ...)`) にして、整数 6 個・浮動小数 8 個のレジスタと
スタックの 16 語を渡す (可変長の関数には `al` = 8 が渡る)。スタックに渡る引数 (レジスタが足りないスカラー、MEMORY の集成型) は
ABI の順に語へ並べる。結果は RAX・RDX・XMM0・XMM1 を `#[repr(C)]` の 2 語の構造体として受ける。`:dynamic` と `:addr` (`dlsym`) も扱う。
コールバックはインタプリタを再入させる必要があるので **扱わない** (実行時エラー)。それを使うテストは `skip-backend: interp` を付ける。
テスト用の C の補助関数は、テストと同じ名前の `.c` を共有ライブラリにしてインタプリタのプロセスに読み込み (`ffi::load_library`)、
ネイティブでは CLI と同じリンカのフラグ (`-L`・`-l`・`-Wl,-rpath`) でリンクする。

**errno。** ランタイムの `cirka_errno` (スレッドの `errno`) を C の関数の直後に呼ぶ (FEAT-0242 で std.os が使う)。
aCHC は `Foreign.C.Error.getErrno` がこれを呼ぶ。

**aCHC。** `foreign import ccall "&sym" p :: FunPtr t` (または `Ptr t`) は `:addr`、`"dynamic"` は `:dynamic`、
`"wrapper"` は `:callback` と、Haskell の関数を引数の箱に入れて呼ぶ (結果が `IO` なら実行する) ハンドラになる。
`freeHaskellFunPtr` は `cirka_callback_free` を呼ぶ。

- Core の名前は取り込む Haskell の名前から作る: `M.f$addr`・`M.f$dynamic`・`M.f$wrapper` (extern) と `M.f$handler` (ハンドラ)。
- `FunPtr ft` のアドレスは、`ft` の型から C シグネチャを作れればそれを付ける (作れなければ `() -> void` の関数として扱う)。`Ptr t` はデータ。
- `wrapper` の環境は `Any` に cast した Haskell の関数。ハンドラは単相でなければならないので、取り込みの型変数は `()` に置き換えて
  ハンドラを作る (型は消去されるので値の扱いは変わらない)。`ft -> IO (FunPtr ft)` の形でない型は E0618。
- `Foreign.C.Error` (縮小版): `Errno`・`getErrno`・`errnoToIOError`・`throwErrno` 系と主な `e…` 定数。`resetErrno` はない
  (インタプリタはスレッドの errno を C の呼び出しの前後で受け渡すため、C 側の `errno` を直接書き換えられない)。

### 7.3 リンクするライブラリと CirkaLang の extern ブロック (FEAT-0250)

**リンクするライブラリ。** 共通 Core の `Program` と MIR の `Module` に、プログラムが使う C のライブラリの名前の並び `libs` を持たせる
(フロントエンドが設定する。テキスト形式には含めない)。

- ネイティブでは、ドライバが CLI のリンカのフラグ (`-L`・`-l`・`-Wl,`) の後に `-l名前` を並べる (`native::lib_args`)。
- インタプリタは実行の前に `lib名前.so` を読み込む (`ffi::load_library`)。C ライブラリの一部 (`c`・`m`・`pthread`・`dl`・`rt`) は
  インタプリタのプロセスに読み込み済みで、その `.so` はリンカスクリプトなので飛ばす。
- CirkaLang は extern の関数のブロックの `link "名前"` を最初に現れた順に重複なく集める ([spec-v0.0.6.md](cirkalang/spec-v0.0.6.md) §1.6)。
  aCHC の `libs` は空 (リンクするライブラリは CLI の `-l` で指定する)。

**CirkaLang の extern ブロックから共通基盤へ。** 宣言の意味は spec-v0.0.6.md §1。共通基盤への写し方は次のとおり。

- 構造体・共用体・列挙・定数・型の別名は、構文解析の後に AST の上で通常のデータ型・定義・`impl Storable` に展開する (`cirkalang-syntax` の `cextern`)。
  構造体・共用体には C のメモリとの間で値を読み書きする `S.$peek : Int -> IO S` と `S.$poke : Int -> S -> IO ()` を生成し、
  フィールドの位置は `Foreign.primCLayout (nullPtr : Ptr S) k` (0 は大きさ、1 は境界、`2 + i` はフィールド `i` の位置) で書く。
  lowering はプログラムの構造体・共用体を §7.2 の C の集成型の表に入れ、`cirka_core::CTypes::layout` で求めた値で `primCLayout` を定数に畳む。
- 値渡しの集成型の引数は、呼び出しの前に `malloc` した領域へ `$poke` で書き、そのアドレスを §7.2 の規則で渡して、呼び出しの後に `free` する。
  集成型の結果は確保した領域に受けて `$peek` で読む。列挙は表現の整数で渡る。
- 種類は §7.2 の表に写す: `"dynamic"` は `:dynamic`、`"&sym"` は `:addr`、`"wrapper"` は `:callback h` で、ハンドラ `h` は
  `{名前}$handler` (CirkaLang の関数を環境として呼び、結果が `IO` なら実行する)。記号名や種類のある宣言の Core の名前は `{名前}$c`。
  同じ記号の宣言は 1 つの C の宣言にまとめ、型が違えば E1271 (可変長の関数は固定部と結果を比べる)。
- C のポインタが指す値の読み書き・`alloca` 系は `Foreign` (core) の `Storable` で、新しい Core の構文は要らない。
  コールバックの解放は `Foreign.freeFunPtr` (`cirka_callback_free`)。

## 8. CirkaLang の記法

```
extern "C" link "c" where
  type CFile                                               -- 不透明型

  fopen  : CString -> CString -> IO (Nullable (OwnedPtr CFile))  free with fclose
  fclose : (1 f : OwnedPtr CFile) -> IO CInt
  write  : CInt -> BorrowedPtr r UInt8 -> CSize -> IO CSSize

  struct Timespec where
    tv_sec  : CLong
    tv_nsec : CLong

  clock_gettime : CInt -> MutPtr Timespec -> IO CInt

-- 安全なラッパ
data File : LType where
  MkFile : (1 p : OwnedPtr CFile) -> File

impl Release File where
  release (MkFile p) = ignore (fclose p)
```

- 型の対応・所有権の考え方 (§2〜§5) は aCHC と同じ。`OwnedPtr` は `LType`。
- **v0.0.2 で実装した範囲** (FEAT-0213、[spec-v0.0.2.md](cirkalang/spec-v0.0.2.md) §8): スカラーの引数と結果、`Foreign` の C の型 (`Int` / `Float` を包むデータ型)、
  `Ptr` / `OwnedPtr`、構成子とフィールドが 1 つの利用者の型、`withCString` / `peekCString`。フロントエンドは引数の箱を開けて結果を箱に入れる Core のラッパと、
  C シグネチャ付きの `extern` 宣言 (§7.1 と同じ共通基盤) を生成する。
- **v0.0.6 で実装した範囲** (FEAT-0250、[spec-v0.0.6.md](cirkalang/spec-v0.0.6.md) §1、§7.3): ブロック内の不透明型 `type T`・型の別名・定数・列挙・構造体・共用体、
  構造体・共用体の値渡し、可変長引数、関数ポインタ (`"dynamic"`・`"wrapper"`・`"&sym"`)、記号名の指定、任意のライブラリの `link`、`Storable`、`cirka bindgen`。
- 上の例の `free with`・`BorrowedPtr`・`MutPtr`・`Nullable`・`CString` は設計の構想で、CirkaLang にはまだない。v0.0.6 ではポインタはすべて `Ptr` で、
  所有権は推測せず (ADR-093)、`Release` を持つ線形な型のラッパを書いて RAII にする (spec-v0.0.6.md §1.8 の zlib)。借用ポインタは借用の領域とともに第二段階で検討する。
- CirkaLang ではエラーは `Result` で返すラッパを標準とし、`errno` は `Result IOError a` に変換する (std.os、FEAT-0242)。
- 部分集合型を FFI 宣言に付けられる (例: `{n : CSize | n <= length buf}`)。宣言の述語は信頼される仮定として扱い、`--report` に列挙する (13 §1 P5)。

## 9. 実用目標のための FFI 要件

| 用途 | 必要な機能 | 担当 |
|------|------------|------|
| CUI / ファイル | `FILE*`・fd を RAII で扱う、`errno` → `Result` | FEAT-0213, 0242 |
| サーバ | ソケット fd、`epoll`、ノンブロッキング I/O、スレッドから安全に呼べること (`Send` 相当の区別) | FEAT-0046, 0240, 0241 |
| syscall | libc の syscall ラッパ、シグナルハンドラ (コールバック)、`mmap` | FEAT-0242 |
| Vulkan (Docker 内は compute のみ。GUI は遠隔環境で任意、[15-gpu-and-remote.md](15-gpu-and-remote.md)) | 構造体・共用体の値渡しとアラインメント、ポインタの配列、ハンドル型 (64bit 不透明値) を `LType` として RAII 管理、関数ポインタの取得 (`vkGetInstanceProcAddr`)、コールバック (デバッグメッセンジャ)、大きなヘッダの bindgen | FEAT-0047, 0048, 0250, 0251, 0290 |
| 数値計算 | BLAS/LAPACK への配列の受け渡し (借用ポインタ、行優先/列優先) | FEAT-0252 |
