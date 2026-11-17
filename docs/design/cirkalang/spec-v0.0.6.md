# CirkaLang v0.0.6 言語仕様

- 版: v0.0.6 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0250 (FFI の完成と bindgen の利用)、FEAT-0255 (大きな extern のモジュールのコンパイル時間)、FEAT-0256 (非公開の構成子)、FEAT-0251 (Vulkan compute)、FEAT-0252 (数学・線形代数)、FEAT-0257 (`Linalg` のコピーの除去)、FEAT-0254 (一意な配列と文字列のビューの大きさ)、FEAT-0253 (リリース)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.6: Vulkan compute・数値計算」を確定したもの。

v0.0.6 の目的は、**実用の C ライブラリ (zlib・Vulkan など) を安全に包めるようにすること** である。
C の構造体・共用体・列挙・定数・関数ポインタ・コールバックを extern ブロックで宣言でき、`cirka bindgen` が C のヘッダからその宣言を書く。
ABI は共通基盤 ([08-ffi.md](../08-ffi.md) §7.2、FEAT-0047)、ヘッダの読み込みは共通の bindgen (§6、FEAT-0048) を使う。
この文書には [spec-v0.0.5.md](spec-v0.0.5.md) からの差分だけを書く。書いていないことは v0.0.5 のとおり。
各節は対応するチケットの完了時に書き足す。

## 0. 変更の一覧

| 節 | 内容 | チケット |
|----|------|----------|
| §1.1 | extern ブロックの宣言: 不透明型・型の別名・定数・列挙・構造体・共用体、記号名の指定 | FEAT-0250 |
| §1.2 | C の型の追加 (`CBool`・`CArray`・`FunPtr`) と `Storable` | FEAT-0250 |
| §1.3 | 構造体と共用体の値: 構築子・フィールドの読み出しと置き換え・値渡し | FEAT-0250 |
| §1.4 | 可変長引数 (`...`) | FEAT-0250 |
| §1.5 | 関数ポインタの呼び出し (`"dynamic"`)・コールバック (`"wrapper"`)・記号のアドレス (`"&sym"`) | FEAT-0250 |
| §1.6 | 任意のライブラリのリンク (`link "z"`) | FEAT-0250 |
| §1.7 | `cirka bindgen` | FEAT-0250 |
| §1.8 | サンプル: zlib を RAII で包む | FEAT-0250 |
| §1.9 | 実行時の表現とコスト、制限、診断 | FEAT-0250 |
| §1.10 | extern ブロックの項目は使うものだけを検査・変換する、`--extern-items`、`check` は Core を最適化しない | FEAT-0255 |
| §2 | 非公開の構成子 (`private` を構成子に付けて、型を他のモジュールから抽象型にする) | FEAT-0256 |
| §3 | Vulkan compute: `Vulkan.Sys` (bindgen) と安全なラッパ `Vulkan` (リージョンによる資源の管理・シェーダ・部分集合型によるワークグループ数・検証レイヤー) | FEAT-0251 |
| §4 | 数学・線形代数: libm の純粋な関数 `Std.Math`、行列 `Matrix m n` と OpenBLAS / LAPACKE による `Linalg` (次元は型にあり、`gemm` は一意なら in-place)、unboxed 配列と C のメモリの間のコピー (`pokeUArray`・`peekUArray`・`peekUArrayInto`)、配列の領域をコピーせずに C に渡す関数 (`withUArrayPtr`・`newUArrayWith`・`modifyUArrayWith`。`Linalg` はこれで BLAS / LAPACK を呼ぶ) | FEAT-0252、FEAT-0257 |
| §5 | 一意な配列の大きさと文字列のビュー: 大きさを型の添字に持つ一意な配列 `Sized n a` (添字を示して検査なしに読み書きする `readS`・`writeS` など)、unboxed 配列の `index` と大きさの契約、文字列のビュー `Str` の不変条件・示された添字で読む `index`・長さの契約、構成子の不変条件を分解していない値にも仮定する検証器 | FEAT-0254 |

## 1. FFI の完成 (FEAT-0250)

### 1.1 extern ブロックの宣言

```
extern "C" link "z" where
  type ZInternal                               -- 不透明型
  type Bytef = CUChar                          -- 型の別名
  const MAX_WBITS : CInt = 15                  -- 定数
  enum Flush : CInt where                      -- 列挙
    Z_NO_FLUSH = 0
    Z_FINISH = 4
  struct ZStream where                         -- 構造体 (packed struct も書ける)
    next_in : Ptr Bytef
    avail_in : CUInt
    ...
  union Number where                           -- 共用体
    i : CInt
    f : CFloat

  deflate : (strm : Ptr ZStream) -> (flush : CInt) -> IO CInt
  "deflateInit_" deflateInit : Ptr ZStream -> CInt -> Ptr CChar -> CInt -> IO CInt
```

- ブロックの項目は、関数の宣言 `名前 : 型` (v0.0.2) に加えて、次の宣言である。先頭の語 `type` `const` `enum` `struct` `union` `packed` は
  extern ブロックの項目の先頭でだけ働く文脈キーワードで、予約語ではない。
  - `type T`: **不透明型**。構成子のない型 `T : Type` で、`Ptr T` の型引数にだけ使える (値として渡すと E1270)。
    C のハンドル (`VkInstance` = `struct VkInstance_T *` など) は不透明型へのポインタで、`Release` を持つ線形な型に包んで RAII で管理する (§1.8)。
  - `type T = 型`: **型の別名** (C の `typedef`)。`T : Type` の大域定義で、定義的等価性で展開される (別の型にはならない)。
    別名は extern の関数・データ型より先に検査するので、どのモジュールの関数の型・構成子のフィールドにも書ける。
  - `const X : 型 = リテラル`: **定数**。型は境界を越えるスカラーの型 (§1.2) か `String`。リテラルは整数・浮動小数・文字列で、数には `-` を付けられる
    (C の型のリテラル、§1.2)。
  - `enum T : 整数の型 where { 名前 = 整数 }`: **列挙** (§1.3)。
  - `struct T where { フィールド : 型 }`、`union T where { … }`、`packed struct T where { … }`: **構造体・共用体** (§1.3)。
- 型・定数・列挙の定数の名前は大文字で始まる (定数も値の名前として使える。パターンには書けない)。フィールドの名前は小文字で始まる。
- `"記号" 名前 : 型` は C の記号 `記号` を `名前` で宣言する (C の名前が予約語や大文字で始まるとき、同じ関数を別の型で宣言するとき)。
  記号は C の識別子 (英数字と `_`) でなければならない (E1272)。記号が `"dynamic"`・`"wrapper"`・`"&…"` のものは §1.5。
  `extern "cirka"` (ランタイムの関数) には記号名も `...` も書けない (E1272)。
- ブロックの宣言は互いに参照でき、書く順序によらない。`private` を付けたブロックの名前 (構成子・フィールドの関数を含む) はすべて非公開になる。
- 局所の宣言は取り込んだモジュールの同名の名前を隠す (bindgen が書く `type Callback = …` は `Foreign.Callback` に優先する)。

### 1.2 C の型と `Storable`

`Foreign` (core) に次を加える。

| 型 | C の型 | 値 |
|----|--------|----|
| `CBool` | `_Bool` (`bool`) | `MkCBool Int` (0 か 1) |
| `FunPtr f` | 関数ポインタ (`f` は関数の型) | `MkFunPtr Int` (アドレス)。`nullFunPtr` `castFunPtr` `funPtrAddress` `castFunPtrToPtr` `castPtrToFunPtr` |
| `CArray n a` | 構造体のフィールドの配列 `a[n]` (`n : Int`) | `MkCArray (List a)` (書き込むとき `n` 個に切り詰め、足りなければ `zeroed` で埋める) |

`Ptr a` と `FunPtr f` の `Eq` `Ord` `Show` は指す先の型によらない (v0.0.5 までの導出は `Eq a` などを要求した)。
`CArray n a` の `Eq` は C の値として比べる: どちらも `n` 個に切り詰め・`zeroed` で埋めた (`resize n`) 要素の列を比べる (`Eq a` と `Storable a` が要る)。

境界を越えるスカラーの型は v0.0.2 の型 (`Int` `UInt` `Float` `Char` `Bool`、`Foreign` の C の型、`Ptr` / `OwnedPtr`、それらを包む型) に `CBool` と `FunPtr` を加えたもの。
構造体・共用体・列挙の型も関数の引数と結果に書ける (§1.3)。

**C の型のリテラル.** 整数リテラルは `Foreign` の C の整数型の値として書ける: `42 : CInt` は `MkCInt 42` である。
範囲は C の型のもの (`CChar` −128〜127、`CUChar` 0〜255、`CShort`・`CUShort`・`CInt`・`CUInt`・`CLong`・`CSSize`・`CULong`・`CSize` も同様、`CBool` 0〜1) で、
外れると E1200 (`literal out of range for `CUChar``)。`CULong`・`CSize` の 2⁶³ 以上の値はビットを保った `Int` になる。
浮動小数点リテラルと整数リテラルは `CFloat`・`CDouble` の値としても書ける。数の演算 (`Num`) は C の型にはない (v0.0.6 では `fromCInt` などで `Int` にして計算する)。

```
trait Storable a where
  sizeOf : Ptr a -> Int            -- 大きさ (バイト)。型はポインタの型で示す: sizeOf (nullPtr : Ptr CInt)
  alignment : Ptr a -> Int
  peek : Ptr a -> IO a
  poke : Ptr a -> a -> IO ()
  zeroed : a                       -- すべてのバイトが 0 の値 (C の {0})
```

- `impl` があるのは、`Foreign` の C の型・`Int` `UInt` `Float` (`int64_t` `uint64_t` `double`)・`Ptr a`・`FunPtr f`・`CArray n a` (`Storable a` から)・
  extern ブロックの構造体・共用体・列挙。読み書きはメモリの位置を検査しない (C と同じ。v0.0.2 の `peekInt32` などと同じ扱い)。
- 補助関数: `peekElemOff` `pokeElemOff` `advancePtr` (要素単位)、`alloca : Storable a => (Ptr a -> IO b) -> IO b`、
  `withValue : Storable a => a -> (Ptr a -> IO b) -> IO b` (`with` は予約語)、`allocaArray` `withArray` `peekArray` `pokeArray` `pokeArrayN`、`mallocArray`、
  `resize : Storable a => Int -> List a -> List a` (`n` 個に切り詰め・`zeroed` で埋める)、`zeroedList`。
  `alloca` などのメモリは関数が戻った後に解放する (`withCString` と同じく、ポインタの持ち出しは検査しない)。

**impl の値の変数.** `impl {n : Int} -> Storable a => Storable (CArray n a)` のように、型でない型 (`Int` など) で宣言した impl の変数は
**実行時に渡される** (型の変数は v0.0.5 までと同じく消去される)。メソッドはその値を使える。
そのインスタンスを使う関数は、変数を多重度 ω で束縛しなければならない (`{ω n : Int} -> CArray n CInt -> …`。多重度 0 の変数から辞書を作ると E0304)。

### 1.3 構造体・共用体・列挙の値

**構造体.** `struct S where { f1 : T1; …; fn : Tn }` は次を定める。

- データ型 `S : Type` と構成子 `MkS : T1 -> … -> Tn -> S` (フィールドの順)。`Eq` と `Show` を導出する。
- フィールドの関数 `S.fi : S -> Ti`、置き換えた値を作る `S.set_fi : Ti -> S -> S`、C のメモリの中のフィールドを指す `S.ptr_fi : Ptr S -> Ptr Ti`。
  名前はデータ型の名前で修飾した形でだけ書ける (別名で取り込んだモジュールなら `Vk.VkExtent2D.width`)。
- `impl Storable S`。フィールドの位置・大きさ・境界は C (x86_64 SysV) と同じで、`packed` は境界を 1 にする。
- フィールドの型は、`Foreign` の C の型・`Int` `UInt` `Float`・`Ptr` `FunPtr`・列挙・構造体・共用体 (値の入れ子。自分自身は不可)・`CArray n T`、それらの別名と、
  それらを包む型 (`Storable` の `impl` が要る)。`Bool` と `Char` (C での大きさが決まっていない。`CBool` を使う)・`String`・リスト・関数・
  線形な型 (`OwnedPtr` を含む) は書けない (E1272)。フィールドのない構造体も書けない (E1272。ポインタでだけ使う型は不透明型にする)。

```
p0 : VkExtent2D
p0 = VkExtent2D.set_width 64 zeroed
```

**共用体.** `union U where { m1 : T1; … }` はデータ型 `U` と構成子 `MkU : List CUChar -> U` で、値は **バイト列** である
(書き込むとき共用体の大きさに切り詰め、足りなければ 0 で埋める)。`impl Storable U` を持つ。
メンバーはポインタを通して読み書きする: `U.mi : Ptr U -> Ptr Ti` (`peek (Number.f p)`)。
`Eq` は大きさのバイトを比べ (`resize`)、`Show` はバイト列を示す。メンバーの型の規則は構造体のフィールドと同じ。

**列挙.** `enum E : I where { A = 1; … }` はデータ型 `E` と構成子 `MkE : I -> E` (`Eq` `Ord` `Show` を導出)、定数 `A : E`、`impl Storable E` を定める。
`I` は `Foreign` の整数の C の型。値は宣言した定数に限らない (C と同じ)。

**値渡し.** 構造体・共用体・列挙の値は、extern の関数の引数と結果にそのまま書ける。構造体と共用体は C の値渡し (x86_64 SysV の分類) で渡り、
列挙は整数として渡る。値は C 側の複製であり、C が書き換えても CirkaLang の値は変わらない。C に書き換えさせるときは `Ptr S` を渡す。
構造体を包む型 (`data W = MkW S`) は境界を越えない (E1270)。

### 1.4 可変長引数

```
extern "C" where
  printf : Ptr CChar -> ... -> IO CInt                       -- 固定部だけで呼ぶ
  "printf" printfIntDouble : Ptr CChar -> ... -> CInt -> CDouble -> IO CInt
```

- 型の中の `...` は可変長の関数の固定部の終わりを表す。`...` の後に書いた引数が、その宣言で渡す可変部である。
  可変部の型を変えて呼ぶときは、記号名を指定して別の名前で宣言する (Idris 2 と同じく、呼び出しごとの型は宣言で決める)。`...` は 1 つだけ書ける (E1273)。
- 可変部には既定の実引数昇格の後の型だけを書ける: `CFloat`・`CChar`・`CUChar`・`CShort`・`CUShort`・`CBool` は E1273 (`CDouble` / `CInt` を使う)。
  構造体と共用体は可変部に書けない (E1273)。

### 1.5 関数ポインタとコールバック

```
extern "C" where
  "dynamic" callVisitor : FunPtr Visitor -> Visitor           -- C の関数ポインタを呼ぶ
  "wrapper" mkVisitor : Visitor -> IO (FunPtr Visitor)        -- CirkaLang の関数を C の関数ポインタにする
  "&getpid" getpidAddr : FunPtr (IO CInt)                     -- 記号のアドレス
```

- `"dynamic"`: 型は `FunPtr F -> F` (`F` は extern の関数として書ける型)。関数ポインタが指す C の関数を `F` として呼ぶ。
- `"wrapper"`: 型は `F -> IO (FunPtr F)`。呼ぶたびに、関数 (クロージャ) を呼ぶ新しい C の関数ポインタを作る。
  ポインタはクロージャを保持し、`freeFunPtr : FunPtr f -> IO ()` (`Foreign`) で解放する (解放しないとクロージャがリークとして報告される)。
  解放した後に C から呼ぶのは利用者の誤り (C の解放済みメモリと同じ)。
- RAII: `Foreign` の線形な型 `Callback f` は `Release` で `freeFunPtr` する。

  ```
  callback : IO (FunPtr f) -> IO1 (Callback f)                 -- cb <- callback (mkVisitor visit)
  callbackPtr : (1 c : Callback f) -> (Callback f, FunPtr f)   -- C に渡すポインタ
  ```

- `"&sym"`: 型は `FunPtr F` か `Ptr T`。記号 `sym` (関数か大域変数) のアドレス。
- 形が違う宣言は E1272。コールバックはランタイムの知っているスレッドから呼ばれなければならない。コールバックの中のパニックはプロセスを終える。
  可変長のコールバックは作れない (08-ffi.md §7.2 の制約)。MIR インタプリタはコールバックを作れない (`-- skip-backend: interp` のテストにする)。

### 1.6 ライブラリのリンク

`extern "C" link "z" where …` の `link` に任意のライブラリの名前を書ける (v0.0.2 の `"c"` `"m"` `"pthread"` だけという制限をなくす)。
`link "a" link "b"` と複数書ける。名前は英数字と `_` `.` `+` `-` だけからなる (E1007)。
`cirka build` / `run` / `test` は、プログラムの extern 関数のブロックの `link` のライブラリを、最初に現れた順に `-l名前` でリンクする
(CLI の `-L` `-l` `-Wl,` と併用できる)。MIR インタプリタは `lib名前.so` を読み込む (C ライブラリの `c` `m` `pthread` `dl` `rt` は読み込み済みとして飛ばす)。

### 1.7 `cirka bindgen`

`cirka bindgen [options] header.h [-o 出力.cirka]` は C のヘッダから §1.1 の宣言の extern ブロックを書く (共通の bindgen、08-ffi.md §6)。
ポインタはすべて `Ptr` で、所有権は推測しない。生成したモジュールの上に、`Release` を持つ線形な型などの安全なラッパを書く (§1.8)。

- 関数ポインタの型の別名 `type T = FunPtr F` には、呼び出す `"dynamic" dyn_T : T -> F` と作る `"wrapper" wrap_T : F -> IO T` も書く
  (aCHC の `dyn_T` / `wrap_T` と同じ。可変長・不透明型の値渡しを含む `F` は除く)。
- テストは生成したモジュールをフロントエンドで検査し (`tests/bindgen/gen`)、`shapes`・`vulkan` (Docker) の `Main.cirka` を実行する。
  `vulkan_core.h` 全体の出力 (構造体 981・共用体 10・列挙 265・型 1189・定数 1403・関数 1889 の宣言、約 2 万行) も検査を通るが、
  約 50 秒かかる (大きな extern のモジュールのコンパイル時間は FEAT-0255)。

### 1.8 サンプル: zlib を RAII で包む

`tests/cirkalang/run/v006/zlib.cirka` は zlib のストリームを線形な型で包み、圧縮と展開をする。

```
extern "C" link "z" where
  type ZState
  type Bytef = CUChar
  struct ZStream where
    next_in : Ptr Bytef
    avail_in : CUInt
    total_in : CULong
    ...                                      -- z_stream のフィールド (14 個)
  enum Flush : CInt where
    NoFlush = 0
    Finish = 4
  const Z_OK : CInt = 0
  "deflateInit_" deflateInit : Ptr ZStream -> CInt -> Ptr CChar -> CInt -> IO CInt
  deflate : Ptr ZStream -> Flush -> IO CInt
  deflateEnd : Ptr ZStream -> IO CInt
  ...

-- A zlib stream: its state in C memory, ended and freed when released.
data Stream : LType where
  MkStream : Mode -> Ptr ZStream -> Stream

impl Release Stream where
  release (MkStream m p) = do
    r <- if m == Compress then deflateEnd p else inflateEnd p
    freeBytes p
    putStrLn ("released the " ++ show m ++ " stream: " ++ show r)

open : Mode -> IO1 (Result CInt Stream)               -- deflateInit_ / inflateInit_
run : (1 s : Stream) -> List Bytef -> Int -> IO1 (Stream, Result CInt (List Bytef))
run (MkStream m p) input cap = do
  out <- withArray input (\inp => allocaArray cap (\buf => do
    poke (ZStream.ptr_next_in p) inp                     -- C のメモリの中のフィールド
    poke (ZStream.ptr_avail_in p) (MkCUInt (length input))
    ...
    r <- if m == Compress then deflate p Finish else inflate p Finish
    z <- peek p                                         -- 構造体の値として読む
    ...))
  pure1 (MkStream m p, out)
```

- `ZStream` の配置はコンパイラが C と同じに計算し、zlib が検査する (`deflateInit_` は `sizeOf (nullPtr : Ptr ZStream)` = 112 を受け取る)。
- `Stream` は使い終わった経路で (エラーで抜けた経路でも) 解放され、`deflateEnd` / `inflateEnd` と `free` が一度だけ走る。
  CirkaLang の側にリークはない (リーク検査を有効にした実行テスト)。インタプリタは `libz.so` を読み込んで同じ結果を出す。
- Docker イメージは zlib の開発用ファイル (`zlib1g-dev`) を持つ。

### 1.9 実行時の表現とコスト、制限、診断

**表現.** 構造体・共用体・列挙の値は CirkaLang のデータ (フィールドを持つ構成子) である。C のメモリとの間は、生成した `S.$peek : Int -> IO S` と
`S.$poke : Int -> S -> IO ()` が各フィールドを `Storable` で読み書きする。配置は共通基盤 (`cirka_core::CTypes::layout`、08-ffi.md §7.2) が計算し、
生成したコードの中の `Foreign.primCLayout p k` (大きさ・境界・フィールドの位置) は lowering で定数になる。

**コスト.**

- 構造体・共用体を値で渡す呼び出しは、引数ごとに C のメモリを `malloc` し、`$poke` で書いて、アドレスからバックエンドが C の規約で渡し、呼び出しの後に `free` する。
  構造体の結果も同様に確保したメモリに受けて `$peek` で読む。ポインタで渡す API (Vulkan の大半) にはこのコストはない。
  スタックに置く最適化は計測の後に行う (16-performance.md §4)。
- 列挙・定数・フィールドの関数は通常のデータと関数で、特別なコストはない。`"dynamic"` の呼び出しは間接呼び出し 1 回、`"&sym"` は記号のアドレスの定数である。
- `"wrapper"` は呼ぶたびにランタイムのトランポリンを 1 つ確保し、クロージャを保持する (`freeFunPtr` まで)。

**制限.** ビットフィールドは格納単位のフィールドとして書く (bindgen はビットの位置をコメントに書く)。`long double`・可変長のコールバック・
構造体を包む型の値渡しは扱わない。コールバックはネイティブのプログラムでだけ動く。

**診断.**

| コード | 内容 |
|--------|------|
| E1007 | 不明な呼び出し規約、ライブラリの名前でない `link` |
| E1200 | C の型のリテラルが範囲外 |
| E1270 | 境界を越えない型 (不透明型・構造体を包む型の値など) |
| E1271 | 同じ記号の宣言の型が違う (プログラムが使う宣言の間で、§1.10) |
| E1272 | 宣言の誤り: フィールドの型、自分自身を含む構造体、フィールドのない構造体、`"dynamic"` `"wrapper"` `"&sym"` の型の形、記号名、ランタイムの関数の記号名 |
| E1273 | 可変長: `...` が 2 つ、昇格される型・構造体を可変部に書いた、可変長のコールバック |

フィールドの型が誤っている構造体については、その構造体のために生成したコードの誤り (`Storable String` がないなど) は重ねて報告しない。

### 1.10 使われる項目 (FEAT-0255)

C のヘッダ全体から書いた extern ブロック (vulkan_core.h なら構造体 981・共用体 10・列挙 265・定数 1403・型の別名 1189・関数 623、約 2 万行) は、
展開すると数万の定義と impl (各構造体のフィールドの関数・`$peek`・`$poke`・`Storable`・`Eq`・`Show`) になるが、プログラムが使うのはその一部である。

**使う項目.** ソース (とライブラリ) に書いた項目はすべて使う。使う項目が名前を書いた大域 (関数・構成子・型・extern の関数) も使う。
データ型を使えば、その構成子のフィールドの型も、その型のために展開した impl も使う。インスタンス探索が選んだ impl も使う。

**検査.**

- 構造体・共用体から展開した項目 (フィールドの関数、`$peek` / `$poke`、`Storable`・`Eq`・`Show` の impl) は、**使うものだけ** をエラボレーションする。
  これらは宣言が §1.9 の検査を通れば型が合うように作られるので、使わないものを検査しても誤りは見つからない。
- 型の別名・定数・列挙 (定数と `Storable`・`Eq`・`Ord`・`Show`)・extern の関数の型は、使わなくても検査する。
  これらの誤り (リテラルが範囲外、列挙の表現の型が `Storable` でないなど) は宣言の誤りだからである。
- 使わない項目は Core に変換しない。同じ記号の宣言の食い違い (E1271) も、使う extern の関数の間で調べる。

**`--extern-items=all`** (既定は `used`) は使わない項目もエラボレーションし、Core に変換する。
展開するコード自体の検査に使う (bindgen の出力を検査するテストはこれを付ける)。

**`cirka check` は Core を最適化しない** (コード生成しない、01-architecture.md §5)。`-O1` / `-O2` を明示すれば最適化して検査する。

**計測.** vulkan_core.h 全体の出力を import するプログラムの `cirka check` は (Docker のリリースビルド)、何も使わなければ 1.9 s・446 MB
(FEAT-0250 の時点では 51.4 s・666 MB)、106 フィールドの `VkPhysicalDeviceLimits` を含む構造体を読み書きするプログラムで 2.7 s。
内訳は構文解析と名前解決 1.1 s・エラボレーション 0.4〜0.8 s・Core への変換 0.2〜0.3 s で、段階ごとの時間は `--time-passes` で表示する。

## 2. 非公開の構成子 (FEAT-0256)

C のハンドルやポインタを包む型は、構成子を他のモジュールに見せると、利用者が任意のポインタから値を作れ、包んだ関数を通してメモリ安全性を壊せる。
構成子の前に `private` を書くと、その構成子は宣言したモジュールの中でだけ使える。型と公開の構成子・関数は従来どおり他のモジュールから使える。

```
data Buffer (s : Region) (e : Type) = private MkBuffer VkBuffer (Ptr e) Int

data File : LType where
  private MkFile : Int -> File
  Closed : File
```

- 短い形では各構成子の前に、GADT の形では行の先頭に書く。`private MkA, MkB : T` は同じ行の構成子すべてに付く。
  データ型の宣言に付けた `private` (型ごと非公開) は従来どおりで、構成子もすべて非公開になる。
- 他のモジュールで非公開の構成子を **式にもパターンにも書けない** (修飾してもしなくても E1106、「`M.C` is private」)。
  パターンで書いた場合も、その中の変数は束縛したものとして扱い、変数の使用について誤りを重ねない。
- 他のモジュールの `case` はワイルドカードや変数でしか値を分解できない。値の中身は宣言したモジュールの関数を通して読む。
- 導出 (`deriving`) は宣言したモジュールで展開されるので、非公開の構成子の型にも働く (`Show` は構成子の名前を表示する)。
- 実行時の表現・コストは変わらない (可視性は名前解決だけの規則である)。

| コード | 内容 |
|--------|------|
| E1106 | (追加) 他のモジュールの非公開の構成子を式・パターンに書いた |

## 3. Vulkan compute (FEAT-0251)

Docker 内 (GPU なし、Mesa lavapipe) で、Vulkan のコンピュートシェーダによる計算を CirkaLang から安全に行う。
ウィンドウと描画は扱わない ([15-gpu-and-remote.md](../15-gpu-and-remote.md))。

### 3.1 ライブラリ

ライブラリの根 `lib/cirkalang/vulkan/` (core・std に続く 3 つ目) に次の 2 つのモジュールを置く。

| モジュール | 内容 |
|------------|------|
| `Vulkan.Sys` | `cirka bindgen --module=Vulkan.Sys --link=vulkan` が vulkan_core.h (1.3.275) から書いた extern 宣言 (構造体 981・共用体 10・列挙 265・定数 1403・別名 1189・関数 623)。生成物をコミットし、bindgen のテストが生成し直した結果と一致することを確かめる (`CIRKA_BLESS=1` で更新)。ポインタはすべて `Ptr` で安全ではない。使う項目だけが検査・変換される (§1.10) |
| `Vulkan` | `Vulkan.Sys` の上の安全なラッパ (以下) |

```
import Foreign
import Data.Borrow (Region)
import Vulkan as Vk

-- c = a + b
add : (0 s : Region) -> Vk.Gpu s -> Vk.VkIO s (List CFloat)
add s g = do
  shader <- Vk.spirvFile "vadd.spv"                -- glslc -o vadd.spv vadd.comp
  a <- Vk.fromList g (map (\i => MkCFloat (toFloat i)) (range 0 999))
  b <- Vk.fromList g (map (\i => MkCFloat 1.0) (range 0 999))
  c <- Vk.buffer g 1000
  p <- Vk.pipeline g shader 3 1                    -- バッファ 3 つ、プッシュ定数 1 つ
  Vk.dispatch g p (Vk.arg a :: Vk.arg b :: Vk.arg c :: Nil) (Vk.PushInt 1000 :: Nil) 16 1 1
  Vk.read c

main : IO ()
main = do
  r <- Vk.withGpu (Vk.options "add") add
  case r of
    Ok xs => print (length xs)
    Err e => print e
```

### 3.2 API

| 名前 | 型 | 内容 |
|------|----|------|
| `withGpu` | `Options -> ((0 s : Region) -> Gpu s -> VkIO s a) -> IO (Result VkError a)` | デバイスを用意して計算を実行し、作ったものをすべて破棄する (§3.3) |
| `options` / `MkOptions` | `String -> Options` / `String -> Bool -> Options` | アプリケーション名と検証レイヤーの有無 (`options` は有効) |
| `VkIO s` | `Functor`・`Monad` | リージョン `s` のデバイスを使う計算。最初の誤りで止まる |
| `io` / `throw` | `IO a -> VkIO s a` / `VkError -> VkIO s a` | `IO` の動作を含める / 誤りで止める |
| `VkError` | `Failed String Int \| Invalid String` | Vulkan の呼び出しの失敗 (関数名と `VkResult`) / このモジュールの誤った使い方 |
| `gpuName` / `gpuKind` / `validated` | `Gpu s -> String` / `Gpu s -> String` / `Gpu s -> Bool` | デバイスの名前・種類 (`discrete GPU` … `CPU`)・検証レイヤーの有無 |
| `buffer` | `Storable e => Gpu s -> Int -> VkIO s (Buffer s e)` | 要素 `n` 個 (`n >= 1`) のバッファ。中身は 0 |
| `fromList` / `write` / `read` / `size` | | リストからバッファを作る / 先頭に書く (要素が多すぎれば `Invalid`) / 全要素を読む / 要素数 |
| `spirvFile` | `String -> VkIO s Shader` | ファイルの SPIR-V (マジックナンバーと大きさを確かめる) |
| `spirv` / `spirvWords` | `Ptr CUInt -> Int -> Shader` / `List CUInt -> Shader` | C のメモリにある SPIR-V (バイト数を添える) / ワードのリスト |
| `pipeline` | `Gpu s -> Shader -> Int -> Int -> VkIO s (Pipeline s)` | バッファ `nb` 個とプッシュ定数 `np` 個 (32 ビット) をとるシェーダのパイプライン |
| `arg` / `Push` | `Storable e => Buffer s e -> Arg s` / `PushInt Int \| PushFloat Float` | `dispatch` に渡すバッファとプッシュ定数 |
| `dispatch` | `Gpu s -> Pipeline s -> List (Arg s) -> List Push -> {x : Int \| 1 <= x && x <= 65535} -> {y : …} -> {z : …} -> VkIO s ()` | `x * y * z` 個のワークグループで実行し、完了を待つ。バッファとプッシュ定数の数がパイプラインと違えば `Invalid` |

- シェーダのエントリーポイントは `main`。set 0 の binding 0, 1, … に記憶バッファ (`dispatch` の引数の順)、プッシュ定数に 32 ビットの値
  (`PushInt` は `int` / `uint`、`PushFloat` は `float`) を順に宣言する。要素の型はシェーダの `std430` の配置と同じものを使う
  (`CFloat` = `float`、`CUInt` = `uint`、`CInt` = `int`)。
- 計算に使うデバイスは、計算のキューを持つものから種類で選ぶ (discrete > integrated > virtual GPU > CPU > その他)。

### 3.3 資源の管理

- `withGpu` は新しいリージョン `s` (spec-v0.0.3.md §3) を作り、インスタンス・デバイス・コマンドプールを作る。
  資源を作る関数はすべて、その破棄の処理をリージョンの中の一覧 (`RefIn s (List (IO ()))`) の先頭に積む。
  `withGpu` は計算が終わると、成功しても誤りで止まっても、一覧を先頭から (作った逆順に) 実行する。
  依存する資源 (バッファ・パイプライン) は依存される資源 (デバイス・インスタンス) より必ず先に破棄される。
- `Gpu s`・`Buffer s e`・`Pipeline s`・`Arg s` は型に `s` を含むので、`withGpu` の結果にできない (E1200)。
  したがって **破棄の後に資源を使うことも、破棄されない資源もない**。利用者は破棄を書かない。
- これらの型の構成子は非公開 (§2) で、利用者がポインタや他のハンドルから資源を作ることはできない (E1106)。
- バッファはホストから見えて一貫性のある (host-visible・coherent) メモリに作り、作成時にマップしたまま、破棄の直前にアンマップする
  (マップは利用者に見えない)。`write` / `read` はマップしたメモリとリストの間でコピーする。
- `dispatch` は記述子プール・記述子セット・コマンドバッファ・フェンスを呼び出しごとに作って破棄する (同期実行)。
  コマンドバッファの最後にシェーダの書き込みをホストの読み出しに見せるバリア (COMPUTE_SHADER → HOST、SHADER_WRITE → HOST_READ) を置くので、
  `dispatch` の後の `read` はシェーダが書いた値を読む。

### 3.4 シェーダの用意

- GLSL から SPIR-V への変換は `glslc` で行う (CirkaLang から SPIR-V を生成することは対象外)。
- `spirvFile` は `glslc -o shader.spv shader.comp` が書いたファイルを読む (C は要らない)。
  `spirv` は `glslc -mfmt=c` が書いた配列を C のファイルに `#include` してリンクし、そのアドレスを `"&name"` (§1.5) で受け取って使う
  (実行ファイルにシェーダを埋め込む)。ポインタとバイト数は extern の宣言と同じく信頼する。
- テストでは、テストの隣の `<テスト>.<名前>.comp` をテストハーネスが `glslc -mfmt=c` で `<名前>.spv.inc` にし、C ヘルパーが `#include` する
  (`CIRKA_GLSLC`。`-- requires: vulkan` のテストは glslc か Vulkan のデバイスがなければ飛ばす)。

### 3.5 部分集合型による制約

- `dispatch` のワークグループ数は `{x : Int | 1 <= x && x <= 65535}` である。65535 はすべての Vulkan の実装が保証する
  `maxComputeWorkGroupCount` の最小値で、0 や大きすぎる数は型検査の誤りになる (E0501)。リテラルは評価で決まり、計算した数は経路の条件などから Z3 で示す
  (spec-v0.0.4.md §3)。部分集合型は消去されるので、実行時のコストはない。
- 評価: 定数の範囲のような **静的に決まる仕様の制約** は部分集合型で自然に書け、利用者の負担も小さい
  (リテラルなら何も書かなくてよく、計算した数は `if` で確かめれば示せる)。一方、バッファの要素数・`dispatch` に渡すバッファの数のように
  別の値 (パイプライン・デバイス) に依存する制約を依存型で書くと、それらの値を型に持ち回る必要があって API が重くなるので、実行時に確かめて `Invalid` を返す。
  デバイスごとの実際の上限は実行時の値なので部分集合型にできず、仕様の下限を使っている。

### 3.6 検証レイヤー

- `options` は検証レイヤー `VK_LAYER_KHRONOS_validation` を有効にする (インストールされていなければ `Invalid`)。`MkOptions name False` で無効にできる。
- 検証レイヤーは誤った使い方 (破棄されていない資源・誤った引数など) を **標準出力** に書く。テストは標準出力を期待出力と比べるので、報告があれば失敗する。
  破棄の処理をわざと抜くと、`vkDestroyDevice` で「`VkBuffer` … has not been destroyed」が報告されてテストが失敗することを確かめた。

### 3.7 実行時の表現とコスト、制限、診断

- `VkIO s a` は `RIO s (Result VkError a)` を包む型で、`>>=` は `Err` で止まる。ハンドルは C のポインタを包む値である。
- MIR インタプリタでも実行できる (`link "vulkan"` の libvulkan を `dlopen` する、spec-v0.0.6.md §1.6)。
- 制限: 同期実行だけ (`dispatch` ごとにコマンドバッファとフェンスを作る)。キューは 1 つ。バッファはホストから見えるメモリだけで、デバイス専用のメモリへの転送はない。
  画像・サンプラー・グラフィックスのパイプライン・ウィンドウは対象外。

| コード | 内容 |
|--------|------|
| E1200 | (既存) 資源を `withGpu` の外に出した |
| E1106 | (既存) 非公開の構成子 (`Vk.MkBuffer` など) を使った |
| E0501 | (既存) ワークグループ数が 1 から 65535 の外にある |

## 4. 数学・線形代数 (FEAT-0252)

C の成熟した数値計算のライブラリ (libm・OpenBLAS・LAPACKE) を、純粋な関数として安全に使う ([09-libraries.md](../09-libraries.md) §1.5)。
再実装はしない。

### 4.1 `Std.Math`

`lib/cirkalang/std/Std/Math.cirka` は、libm の関数を `extern "C" link "m"` で宣言した純粋な関数 (§1.1) と定数を持つ。

| 名前 | 内容 |
|------|------|
| `sin` `cos` `tan` `asin` `acos` `atan` `atan2` | 三角関数 (`atan2 y x` は点 (x, y) の角度で、-π から π) |
| `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | 双曲線関数 |
| `exp` `exp2` `expm1` `log` `log2` `log10` `log1p` `pow` `cbrt` `hypot` | 指数・対数・べき (`expm1`・`log1p` は 0 の近くで精度を保ち、`hypot` はあふれない) |
| `floor` `ceil` `round` `trunc` `fmod` `fma` `copysign` `fmin` `fmax` | 丸め (結果は `Float`。`round` は半分を 0 から遠ざける)・`x` の符号の剰余・丸めが 1 回の積和など |
| `pi` `euler` `infinity` `nan` | 定数 |
| `isNaN` `isInfinite` `isFinite` | 判定 |

- 引数と結果は `Float` (C の `double`)。値は glibc の libm のもので、正しく丸めた値とは限らない (`cbrt 27.0` は 3.0000000000000004)。
  定義域の外は NaN、極は無限大になり、プログラムは止まらない (`errno` は読まない)。
- ネイティブのプログラムもインタプリタも glibc の関数を呼ぶ (ランタイムの中の compiler-builtins の同名の関数ではない。BUG-0028)。
- `sqrt`・`abs`・`truncate` は Prelude にある。

### 4.2 `Linalg`

ライブラリの根 `lib/cirkalang/linalg/` (core・std・vulkan に続く 4 つ目) の `Linalg` は、`Float` の行列と、OpenBLAS (CBLAS) と LAPACKE による線形代数を持つ。

```
import Linalg

a : Matrix 2 3
a = matrix 2 3 (\i, j => toFloat (i + j))

main : IO ()
main = do
  print (mul a (transpose a))                          -- Matrix 2 2 (dgemm)
  print (solve (identity 2) (matrix 2 1 (\i, j => 1.0))) -- Just (dgesv)
  print (at a 1 2)                                     -- 添字は型の次元に対して検査時に示す
```

| 名前 | 型 | 内容 |
|------|----|------|
| `Matrix m n` | | `m` 行 `n` 列の行列。構成子は非公開 (§2) |
| `Order` | `RowMajor \| ColMajor` | 要素の並び |
| `matrix` | `(m : {m : Int \| 0 <= m}) -> (n : {n : Int \| 0 <= n}) -> (Int -> Int -> Float) -> Matrix m n` | 要素 (i, j) が `f i j` の行列 (行優先) |
| `fromLists` | `(m : …) -> (n : …) -> List (List Float) -> Maybe (Matrix m n)` | 行のリストから作る。大きさが違えば `Nothing` |
| `identity` | `(n : {n : Int \| 0 <= n}) -> Matrix n n` | 単位行列 |
| `rows` / `cols` / `order` | `Matrix m n -> Int` / … / `Matrix m n -> Order` | 行数・列数・並び |
| `at` | `{m : Int} -> {n : Int} -> Matrix m n -> {i : Int \| 0 <= i && i < m} -> {j : Int \| 0 <= j && j < n} -> Float` | 要素 |
| `get` | `Matrix m n -> Int -> Int -> Maybe Float` | 要素 (範囲の外は `Nothing`) |
| `toLists` | `Matrix m n -> List (List Float)` | 行のリスト |
| `transpose` | `Matrix m n -> Matrix n m` | 転置。並びを変えるだけで、要素はコピーしない |
| `withOrder` | `Order -> Matrix m n -> Matrix m n` | 要素を指定の並びにする (違えばコピー) |
| `add` / `sub` / `scale` / `mapElems` / `zipElems` | | 要素ごとの演算 |
| `mul` | `Matrix m k -> Matrix k n -> Matrix m n` | 積 (`cblas_dgemm`)。結果は行優先 |
| `gemm` | `Float -> Matrix m k -> Matrix k n -> Float -> Matrix m n -> Matrix m n` | `alpha * a * b + beta * c` (`cblas_dgemm`)。結果は `c` の並びで、`c` の配列をほかの値が共有していなければその配列に書く |
| `gemmIn` | `Float -> Matrix m k -> Matrix k n -> Float -> (1 c : Unique (Matrix m n)) -> Unique (Matrix m n)` | 一意な行列 `c` の上に `gemm` の結果を書く (in-place 版) |
| `solve` | `Matrix n n -> Matrix n k -> Maybe (Matrix n k)` | `a * x = b` の解 (`LAPACKE_dgesv`、部分ピボット付きの LU 分解)。`a` が特異なら `Nothing`。結果は `a` の並び |
| `inverse` | `Matrix n n -> Maybe (Matrix n n)` | 逆行列 |
| `det` | `Matrix n n -> Float` | 行列式 (`LAPACKE_dgetrf`) |
| `Show` / `Eq` | | 行のリストとして表示・比較する (並びによらない) |

- **次元は型にある。** 型の引数 `m`・`n` は `Int` の値で、型検査の後に消去される (行列は実行時の次元を自分の値としても持つ)。
  積・和などの次元が合わなければ型の誤り (E1200)、`at` の添字が型の次元の範囲にあることは部分集合型として検査時に示す (E0501。spec-v0.0.4.md §3)。
  `matrix`・`identity` は次元の値をとって型に使う依存型の関数で、実行時に決まる `n` (`0 <= n` を示したもの) からも `Matrix n n` を作れる。
- **純粋。** 関数はすべて純粋で、BLAS と LAPACK は行列の配列の領域の上で直接呼ぶ (§4.3。FEAT-0257)。BLAS は入力の配列を読むだけで、
  結果を新しい配列か `c` の配列に書く。LAPACK は入力を上書きする (`solve` の `a` は LU 分解に、`b` は解になる) ので、
  入力の配列をほかの値が共有していればコピーしてから渡し、共有していなければその配列を使う。
  `gemm` が `c` の配列に結果を書くのは、その配列をほかの値が共有していないときだけなので、観測できない (一意な配列の更新と同じ。spec-v0.0.2.md §7)。
  `gemmIn` は一意な行列 (`Unique`) を受け取って返すので、書き換える前の `c` を利用者が使えないことが型で分かる
  (`c` を `unique` で作ったときに、まだ要素を共有する行列が残っていれば、要素はコピーされる)。
- **並び。** 行列は 1 つの unboxed 配列 (`UArray Float`) に行優先 (`RowMajor`) か列優先 (`ColMajor`) で要素を持つ。
  BLAS と LAPACK の呼び出しはどちらの並びも読める (他方の並びは転置として渡す) ので、並びを変えるコピーは `solve` の `b` の並びが `a` と違うときだけ行う。
- LAPACK が不正な引数 (`info < 0`) を報告したら、このモジュールの誤りなので `panic` する。
- テストは OpenBLAS と LAPACKE がなければ飛ばす (`-- requires: lib:openblas lib:lapacke`。`requires:` の `lib:<名前>` は `lib<名前>.so` を読み込めることを求める)。
  Docker のイメージには両方があり、`scripts/ci.sh` が確かめる。

### 4.3 unboxed 配列と C のメモリ

`Foreign` に、unboxed 配列 (spec-v0.0.2.md §7.3) と C のメモリの間で要素をまとめてコピーする関数と、
配列の要素の領域をコピーせずに C に渡す関数 (FEAT-0257) を加える。
要素は 8 バイトで、`Int` は `int64_t`、`Float` は `double` として読み書きする。

| 名前 | 型 | 内容 |
|------|----|------|
| `pokeUArray` | `Ptr a -> UArray a -> IO ()` | 配列の要素を `p` から書く |
| `peekUArray` | `Int -> Ptr a -> IO (UArray a)` | `p` から `n` 個の要素を読んだ新しい配列 |
| `peekUArrayInto` | `UArray a -> Ptr a -> IO (UArray a)` | `p` から `size arr` 個の要素を読んだ配列。`arr` をほかの値が共有していなければ `arr` の領域に書く (共有されていればコピーに書く) |

| `withUArrayPtr` | `UArray a -> (Ptr a -> IO b) -> IO b` | 配列の要素の領域を指すポインタを `k` に渡して実行する (コピーしない)。`k` は読むだけで、書いてはならない (ほかの値が配列を共有しうる) |
| `newUArrayWith` | `Int -> (Ptr a -> IO b) -> IO (UArray a, b)` | `n` 個の要素の新しい配列 (初めはすべて 0) の領域に `k` が書き、その配列と `k` の結果を返す |
| `modifyUArrayWith` | `UArray a -> (Ptr a -> IO b) -> IO (UArray a, b)` | `arr` の領域を `k` が書き換えた配列と `k` の結果。`arr` をほかの値が共有していなければその場で、共有されていればコピーを書き換える (`thaw` と同じ) |

- 値としては、`peekUArrayInto` と `modifyUArrayWith` も新しい配列を返す。元の配列を共有する値からは変化が見えない。
- ランタイムには GC がなくオブジェクトは動かないので、配列の要素の領域のアドレスは配列が生きている間変わらない。
  `withUArrayPtr`・`newUArrayWith`・`modifyUArrayWith` は、`k` が終わるまで配列を解放しないことを保証する。ライブラリの中だけで宣言できる
  プリミティブ `primUArrayAddr : {a : Type} -> UArray a -> Int` (アドレス) と `primUArrayTouch : {a : Type} -> UArray a -> IO ()`
  (何もしない外部呼び出し) を使い、`k` の後で配列を touch する。外部呼び出しは効果として扱われて最適化で消えも動きもしないので、
  配列の解放 (Perceus の `drop`) は touch の後、つまり `k` の後になる。
  - 配列を結果として返す `newUArrayWith` / `modifyUArrayWith` でも touch が要る。呼ぶ側が結果の配列を使わなければ
    (`det` は LU 分解を捨てる)、最適化はアドレスを取った直後に配列を解放するからである (FEAT-0257 の作業で見つけた。
    回帰テストは `tests/cirkalang/run/v006/uarray_ptr.cirka` の最後の `modifyUArrayWith`)。
- `k` はポインタを動作の後まで保持してはならない。ポインタの先で読み書きしてよいのは配列の要素 (`size arr` 個) だけで、
  これは `peek` / `poke` と同じく利用者が保証する。
- `newUArrayWith` / `modifyUArrayWith` が `k` に渡す領域の配列はほかの値と共有されていないので、`k` の書き込みは返す配列にだけ見える
  (一意な配列の更新と同じく観測できない)。
- ポインタの先の大きさは `peek` / `poke` と同じく利用者が保証する。空の配列ではポインタを使わない (`nullPtr` でもよい)。
- ライブラリの中だけで宣言できるプリミティブ `primUnsafeRunIO : {a : Type} -> IO a -> a` は、`IO` の動作を純粋な値として実行する。
  使ってよいのは、動作の効果が自分で確保して解放するメモリの上だけにあって、何回実行しても実行しなくても観測できない場合だけで、`Linalg` が使う。
  プリミティブは利用者のプログラムには書けない (BUG-0023)。

### 4.4 実行時の表現とコスト、制限、診断

- BLAS / LAPACK は配列の領域を直接使い、C のメモリのバッファもそこへのコピーもない (FEAT-0257)。配列のコピーは、
  LAPACK が上書きする入力か `gemm` の `c` をほかの値が共有しているときの 1 回だけである (FEAT-0252 の版の C のバッファへのコピーの代わり)。
  計測では、512×512 の積 20 回が C から `cblas_dgemm` を直接呼ぶ場合の 1.02 倍 (261.8 ms 対 256.8 ms。FEAT-0252 の版は 1.36 倍) である (ADR-098)。
- 配列を書き換えられるか (コピーが要らないか) は実行時の参照カウントで決まり、最適化にもよる。FEAT-0252 の版では、大きな関数の中で
  `gemm` の動作がクロージャとして確保されて `c` の配列を持ち、配列がコピーされた (FEAT-0088。FEAT-0257 の版では現れない)。
  `max-array-copies` のテストで回数を確かめている (`linalg.cirka` は共有された入力の 9 回、`linalg_inplace.cirka` は 0 回)。
- MIR インタプリタでも実行できる (`link` の libopenblas・liblapacke を `dlopen` する。§1.6)。
- 制限: 要素は `Float` (`double`) だけで、単精度・複素数はない。BLAS は level 3 の `dgemm` だけを包み、ベクトルの演算は CirkaLang で書く。
  固有値・特異値分解・最小二乗法などはない。

| コード | 内容 |
|--------|------|
| E1200 | (既存) 行列の次元が合わない |
| E0501 | (既存) `at` の添字が型の次元の範囲にない、次元が負 |
| E1106 | (既存) 非公開の構成子 `MkMatrix` を使った |

## 5. 一意な配列の大きさと文字列のビュー (FEAT-0254)

v0.0.5 では共有の配列を作る関数に大きさの契約を付け、`A.index` の境界検査を証明で除いた (spec-v0.0.5.md §4)。
一意な配列 (`Unique (Array a)`、種 `LType`) は部分集合型を持てない (spec-v0.0.4.md §3.5) ので、in-place の更新 (`read`・`write`) の検査は残っていた。
v0.0.6 では **大きさを型の添字に持つ一意な配列** `Sized n a` を加え、添字を `n` に対して示して検査なしに読み書きする。
文字列のビュー `Str` には不変条件を付け、`Data.String` の繰り返しと、示された添字で読む `index` から境界検査を除く。

### 5.1 大きさを型に持つ一意な配列 (`Sized`)

`Data.Array` と `Data.Array.Unboxed` に次を加える (unboxed では要素を読み書きする関数に `Unbox a` の制約が付き、配列は `UArray a`)。

```
data Sized (n : Int) (a : Type) : LType where
  private MkSized : (1 u : Unique (Array a)) -> Sized n a

newS    : (n : Int) -> a -> Sized n a                   -- unboxed には zerosS : (n : Int) -> Sized n a も
thawS   : (n : Int) -> (arr : Array a) -> Sized n a
  requires size arr == n
readS   : (1 s : Sized n a) -> {i : Int | 0 <= i && i < n} -> (Sized n a, Ur a)
writeS  : (1 s : Sized n a) -> {i : Int | 0 <= i && i < n} -> a -> Sized n a
modifyS : (1 s : Sized n a) -> {i : Int | 0 <= i && i < n} -> (a -> a) -> Sized n a
swapS   : (1 s : Sized n a) -> {i : Int | 0 <= i && i < n} -> {j : Int | 0 <= j && j < n} -> Sized n a
trusted total
freezeS : (1 s : Sized n a) -> Array a
  ensures \r => size r == (if n < 0 then 0 else n)
unsized : (1 s : Sized n a) -> Unique (Array a)
```

- `Sized n a` は `n` 個 (`n` が負なら 0 個) の要素の一意な配列である。`n` は消去され、実行時の値は一意な配列そのもの (箱も検査もない)。
  `LType` の値を部分集合型にする代わりに、大きさを型の添字に持たせて証明に載せる (ADR-100)。
- 構成子は非公開 (§2) で、値を作るのはこのモジュールだけである。`newS` / `zerosS` は作った配列の大きさを確かめ
  (`n` 個の要素が収まらず大きさが合わないときは異常終了する。unboxed の配列はバイト数 `8 * n` がラップしうる)、`thawS` は `size arr == n` を
  呼ぶ側に示させる。`readS` 以下は大きさを変えない。したがって `Sized n a` の配列の大きさはいつも `n` (負なら 0) であり、
  `freezeS` の結果の大きさを信頼する仕様 (spec-v0.0.5.md §4.2) として与える。
- 添字の部分集合型は呼ぶ側が示す (spec-v0.0.4.md §3)。`readS` などは非公開のプリミティブ (`primArrayReadProved`・`primArrayWriteProved`、
  unboxed は `primUArray{Index,Read,Write}{Word,Float}Proved`) で書かれ、証明を信頼するとき (`verify`・`check`) は Core の
  `array_set_unchecked`・`bytes_get64_unchecked` などを経て MIR の検査のない演算になる。`--contracts=off` では検査のある演算になる (spec-v0.0.4.md §5)。
- unboxed の要素の検査のない読み書きは `Unbox` のメソッド `unboxIndexAt`・`unboxReadAt`・`unboxWriteAt` が行う。これらは構成子が非公開の型
  `InBounds` の添字を取るので、利用者は直接は呼べない。
- `Data.Array.Unboxed` にも、共有の配列の示された添字での読み出し `index : Unbox a => (arr : UArray a) -> {i : Int | 0 <= i && i < size arr} -> a` と、
  `fromList`・`generate`・`map` の大きさの契約を加える (`Data.Array` と同じ。spec-v0.0.4.md §5、spec-v0.0.5.md §4.1)。

例 (`tests/cirkalang/run/v006/sized_arrays.cirka`): `writeS` の添字 `i` は、`requires 0 <= i` と `if` の条件 `i < n` から示される。

```
squares : (n : Int) -> (1 s : U.Sized n Int) -> (i : Int) -> U.Sized n Int
  requires 0 <= i
squares n s i = if i >= n then s else squares n (U.writeS s i (i * i)) (i + 1)

main = do
  let sq = U.freezeS (squares 8 (U.zerosS 8) 0)
  print (U.index sq 7)          -- 大きさ 8 は freezeS の契約から分かる
```

### 5.2 文字列のビュー (`Data.String`)

```
data Str : Type where
  MkStr : (s : String) -> (i : Int) -> (j : Int) -> Str
  invariant 0 <= i && i <= j && j <= strLength s

index : (v : Str) -> {k : Int | 0 <= k && k < length v} -> Int

total
toStr : (s : String) -> Str
  ensures \r => length r == strLength s
total
take : (n : Int) -> (v : Str) -> Str
  ensures \r => length r == (if n < 0 then 0 else if n < length v then n else length v)
total
drop : (n : Int) -> (v : Str) -> Str
  ensures \r => length r == (if n < 0 then length v else if n < length v then length v - n else 0)
total
slice : (v : Str) -> (a : Int) -> (b : Int) -> Str
  ensures \r => length r <= length v && (a < 0 || b < a || length v < b || length r == b - a)
```

- ビュー `MkStr s i j` は `s` のバイト `[i, j)` で、範囲が `s` の中にあることを構成子の不変条件 (spec-v0.0.4.md §4.2) にした。
  構成子は公開のままで、利用者が作るときは不変条件を示す。
- `index v k` は、添字が範囲内と示されたバイトを境界検査なしで読む (`Data.Array.index` と同じ)。範囲外で異常終了する `byteAt` も残す。
- `Data.String` の繰り返し (`words`・`lines`・`trim`・`toInt`・`chars`・`splitOn`・`toUpper`・比較など) は、不変条件と補助関数の `requires` から
  読み出しの添字を示し、境界検査なしで読む。文字列の検索 (ランタイムの `cirka$bytes_find`・`cirka$bytes_find_byte`) の結果が範囲の中にあることは
  信頼する仕様にした (検索は範囲を文字列に切り詰めるが、不変条件の下では範囲そのものである)。
- `toStr`・`take`・`drop`・`slice` の結果の長さを契約にした。どれも `total` なので、`let` で束縛した結果にも使える。
  `slice v a b` の長さは、範囲が `0 <= a <= b <= length v` のとき `b - a` と分かる。
- 添字は `Int` のラップで溢れないように計算する (`take n v` の終わりは `i + clamp n 0 (j - i)`)。以前は `slice v a b` の `i + a` が
  溢れると範囲がずれた (`drop maxInt v` が空でなかった)。溢れない引数での結果は以前と同じである。

例 (`tests/cirkalang/run/v006/string_views.cirka`):

```
palindrome : (v : Str) -> (lo : Int) -> (hi : Int) -> Bool
  requires 0 <= lo && hi < S.length v
palindrome v lo hi =
  if lo >= hi then True
  else if S.index v lo == S.index v hi then palindrome v (lo + 1) (hi - 1)
  else False

isPalindrome : Str -> Bool
isPalindrome v = palindrome v 0 (S.length v - 1)    -- S.length v は負でない (§5.3)

year : Str -> Maybe Int                              -- "2026-11-17" の 2026
year v =
  if S.length v < 4 then Nothing
  else
    let y = S.take 4 v in                            -- S.length y == 4
    Just (digit (S.index y 0) * 1000 + digit (S.index y 1) * 100 + digit (S.index y 2) * 10 + digit (S.index y 3))
```

### 5.3 検証器の変更

- 構成子が 1 つの型 (レコード) の値を構成子で分解していなくても、フィールドの部分集合型 (構成子の `invariant`) を、その値の射影
  (spec-v0.0.4.md §3.2 の、分解したフィールドを表す変数) について仮定する。値を作った所で示されているか、実行時に検査されているからである
  (`--contracts=off` では証明を信頼しないので、仮定に頼る検査の除去もない)。これで `S.length v` が負でないことを、`v` を分解せずに使える。
- 結果に部分集合型を持つ `total` な関数の呼び出し (`S.drop n v`) は、それを引数にとる定義 (`S.length`) を展開して調べるときも
  呼び出しのまま扱い、結果の部分集合型を仮定する (spec-v0.0.4.md §3.3。以前は `length` とともに `drop` も展開されて契約が失われた)。
  `total` に限るのは、`size` の引数の呼び出し (spec-v0.0.5.md §4) と同じく、消去される位置の止まらない呼び出しから偽の事実を得ないためである。
- 一つの証明義務の中で、同じ呼び出しは同じ項に訳す (BUG-0031)。定義の展開は深さで打ち切るので、目標と、それより深く訳す仮定 (呼び出しの代わりの
  変数の部分集合型) とで同じ呼び出し (`let n = parseInt arg` の `n`) が別の項になり、`U.thawS n (U.generate n f)` が示せなかった。
  最初の訳を使い回し、新しい仮定は加えない (二つの訳が等しいという仮定は、止まらない呼び出しで矛盾する)。
- 型 (シグネチャと契約) の中では、前に宣言した関数だけを使える (Idris 2 と同じ。関数の本体はどの関数でも使える)。
  後で宣言する関数を使うと E1121 (以前は診断なしに誤りの項になった。BUG-0030)。構成子の不変条件は、シグネチャの後でもう一度検査するので
  後の関数も使える (spec-v0.0.4.md §4.2)。
- 簡約できないプリミティブ (`primUArraySize` など) の引数は評価しない。検査器が値の分からない式を評価する 2 か所 (義務を評価で決める所と、
  呼び出しの引数が辞書かを調べる所) が、一意な配列を作る繰り返しを評価して指数時間かかっていた (BUG-0029)。

### 5.4 実行時の表現とコスト、制限、診断

- 計測 (Docker。[bench-report.md](../bench-report.md) の「一意な配列の大きさと文字列のビューの証明 (FEAT-0254)」):
  `bench/stencil` (100 万要素の 3 点ステンシルを 100 回。読む配列は共有の `UArray`、書く配列は `Sized`) は Rust と同じ時間 (215 ms 対 216 ms) で、
  命令数は `--contracts=off` より 20% 少なく Rust の 1.01 倍。`--contracts=off` ではループの入口の検査 (FEAT-0069) が `i - 1` の読み出しの検査を
  除けない。`bench/string-split` は `Data.String` の読み出しの検査が消えて命令数が 3.7% 減り、時間は Rust の 1.44 倍。
- 証明は実行時に消えるが、`ensures` を持つ関数の結果は `primRefine` (恒等関数) を通る。Core の簡約が呼び出しを返すだけの `let!` を
  呼び出しに戻すので、自己呼び出しは末尾呼び出しのままで、戻り値も unboxed になる (06-ir.md §1.3)。
- 制限:
  - `Sized` の大きさは型の中の `Int` で、繰り返しの添字との関係は呼ぶ側が `requires` で示す。大きさを変える操作 (追加・連結) はない。
  - 一意な文字列 (`Builder` の内部や `toUpper` が書くバッファ) への書き込みの検査は残る。
  - 文字列リテラルの長さは証明から見えない (`S.index (toStr "abc") 1` は示せない。`S.length` を確かめる `if` が要る)。

| コード | 内容 |
|--------|------|
| E1121 | 型 (シグネチャ・契約) の中で、後で宣言する関数を使った (§5.3、BUG-0030) |
