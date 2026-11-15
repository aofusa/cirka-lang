# CirkaLang v0.0.6 言語仕様

- 版: v0.0.6 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0250 (FFI の完成と bindgen の利用)、FEAT-0251 (Vulkan compute)、FEAT-0252 (数学・線形代数)、FEAT-0254 (一意な配列と文字列のビューの大きさ)、FEAT-0253 (リリース)
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
  - `type T = 型`: **型の別名** (C の `typedef`)。`T : Type` の大域定義で、定義的等価性で展開される (別の型にはならない)。
  - `const X : 型 = リテラル`: **定数**。型は境界を越えるスカラーの型 (§1.2) か `String`。リテラルは整数・浮動小数・文字列で、数には `-` を付けられる。
  - `enum T : 整数の型 where { 名前 = 整数 }`: **列挙** (§1.3)。
  - `struct T where { フィールド : 型 }`、`union T where { … }`、`packed struct T where { … }`: **構造体・共用体** (§1.3)。
- 型・定数・列挙の定数の名前は大文字で始まる (定数も値の名前として使える。パターンには書けない)。フィールドの名前は小文字で始まる。
- `"記号" 名前 : 型` は C の記号 `記号` を `名前` で宣言する (C の名前が予約語や大文字で始まるとき、同じ関数を別の型で宣言するとき)。
  記号が `"dynamic"`・`"wrapper"`・`"&…"` のものは §1.5。
- ブロックの宣言は互いに参照でき、書く順序によらない。`private` を付けたブロックの名前 (構成子・フィールドの関数を含む) はすべて非公開になる。

### 1.2 C の型と `Storable`

`Foreign` (core) に次を加える。

| 型 | C の型 | 値 |
|----|--------|----|
| `CBool` | `_Bool` (`bool`) | `MkCBool Int` (0 か 1) |
| `FunPtr f` | 関数ポインタ (`f` は関数の型) | `MkFunPtr Int` (アドレス)。`nullFunPtr` `castFunPtr` `funPtrAddress` `castFunPtrToPtr` `castPtrToFunPtr` |
| `CArray n a` | 構造体のフィールドの配列 `a[n]` (`n : Int`) | `MkCArray (List a)` (書き込むとき `n` 個に切り詰め、足りなければ `zeroed` で埋める) |

`Ptr a` と `FunPtr f` の `Eq` `Ord` `Show` は指す先の型によらない (v0.0.5 までの導出は `Eq a` などを要求した)。

境界を越えるスカラーの型は v0.0.2 の型 (`Int` `UInt` `Float` `Char` `Bool`、`Foreign` の C の型、`Ptr` / `OwnedPtr`、それらを包む型) に `CBool` と `FunPtr` を加えたもの。
構造体・共用体・列挙の型も関数の引数と結果に書ける (§1.3)。

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
  `withValue : Storable a => a -> (Ptr a -> IO b) -> IO b` (`with` は予約語)、`allocaArray` `withArray` `peekArray` `pokeArray`、`mallocArray`。
  `alloca` などのメモリは関数が戻った後に解放する (`withCString` と同じく、ポインタの持ち出しは検査しない)。

**impl の値の変数.** `impl {n : Int} -> Storable a => Storable (CArray n a)` のように、型でない型 (`Int` など) で宣言した impl の変数は
**実行時に渡される** (型の変数は v0.0.5 までと同じく消去される)。メソッドはその値を使える。
そのインスタンスを使う関数は、変数を多重度 ω で束縛しなければならない (`{ω n : Int} -> CArray n CInt -> …`。多重度 0 の変数から辞書を作ると E0304)。

### 1.3 構造体・共用体・列挙の値

**構造体.** `struct S where { f1 : T1; …; fn : Tn }` は次を定める。

- データ型 `S : Type` と構成子 `MkS : T1 -> … -> Tn -> S` (フィールドの順)。`Eq` と `Show` を導出する。
- フィールドの関数 `S.fi : S -> Ti` と、置き換えた値を作る `S.set_fi : Ti -> S -> S`。名前はデータ型の名前で修飾した形でだけ書ける
  (別名で取り込んだモジュールなら `Vk.VkExtent2D.width`)。
- `impl Storable S`。フィールドの位置・大きさ・境界は C (x86_64 SysV) と同じで、`packed` は境界を 1 にする。
- フィールドの型は、境界を越えるスカラーの型・列挙・構造体・共用体 (値の入れ子。自分自身は不可)・`CArray n T` とそれらの別名。
  `String`・リスト・関数・線形な型 (`OwnedPtr` を含む) は書けない (E1272)。

```
p0 : VkExtent2D
p0 = VkExtent2D.set_width (MkCUInt 64) zeroed
```

**共用体.** `union U where { … }` はデータ型 `U` と構成子 `MkU : CArray k CUChar -> U` (`k` は共用体の大きさ) で、値は **バイト列** である。
`impl Storable U` を持ち、メンバーはポインタを通して読み書きする (`peek (castPtr p : Ptr CFloat)`)。

**列挙.** `enum E : I where { A = 1; … }` はデータ型 `E` と構成子 `MkE : I -> E` (`Eq` `Ord` `Show` を導出)、定数 `A : E`、`impl Storable E` を定める。
`I` は `Foreign` の整数の C の型。値は宣言した定数に限らない (C と同じ)。

**値渡し.** 構造体・共用体・列挙の値は、extern の関数の引数と結果にそのまま書ける。構造体と共用体は C の値渡し (x86_64 SysV の分類) で渡り、
列挙は整数として渡る。値は C 側の複製であり、C が書き換えても CirkaLang の値は変わらない。C に書き換えさせるときは `Ptr S` を渡す。

### 1.4 可変長引数

```
extern "C" where
  printf : Ptr CChar -> ... -> IO CInt                       -- 固定部だけで呼ぶ
  "printf" printfIntDouble : Ptr CChar -> ... -> CInt -> CDouble -> IO CInt
```

- 型の中の `...` は可変長の関数の固定部の終わりを表す。`...` の後に書いた引数が、その宣言で渡す可変部である。
  可変部の型を変えて呼ぶときは、記号名を指定して別の名前で宣言する (Idris 2 と同じく、呼び出しごとの型は宣言で決める)。
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
  `freeFunPtr : FunPtr f -> IO ()` (`Foreign`) で解放する。解放した後に C から呼ぶのは利用者の誤り (C の解放済みメモリと同じ)。
  `Foreign` の線形な型 `Callback f` (`Release` で解放する) で包むと RAII で扱える。
- `"&sym"`: 型は `FunPtr F` か `Ptr T`。記号 `sym` (関数か大域変数) のアドレス。
- コールバックはランタイムの知っているスレッドから呼ばれなければならない。コールバックの中のパニックはプロセスを終える。可変長のコールバックは作れない
  (08-ffi.md §7.2 の制約)。

### 1.6 ライブラリのリンク

`extern "C" link "z" where …` の `link` に任意のライブラリの名前を書ける (v0.0.2 の `"c"` `"m"` `"pthread"` だけという制限をなくす)。
`cirka build` / `run` / `test` は、プログラムが取り込むモジュールの `link` のライブラリを、最初に現れた順に `-l名前` でリンクする
(CLI の `-L` `-l` `-Wl,` と併用できる)。MIR インタプリタは同じライブラリを読み込む。

### 1.7 `cirka bindgen`

`cirka bindgen [options] header.h [-o 出力.cirka]` は C のヘッダから §1.1 の宣言の extern ブロックを書く (共通の bindgen、08-ffi.md §6)。
ポインタはすべて `Ptr` で、所有権は推測しない。生成したモジュールの上に、`Release` を持つ線形な型などの安全なラッパを書く (§1.8)。

### 1.8 サンプル: zlib を RAII で包む

(実装時に書く)

### 1.9 実行時の表現とコスト、制限、診断

(実装時に書く)
