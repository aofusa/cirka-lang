# 08. C FFI

`docs/specs/01-cffi.md` の構想(仮称 LinHask)を具体化したもの。
目標は「宣言は Rust/Swift/cgo 並みに簡単、安全性は線形型で保証、オーバーヘッドはゼロ」。

ABI の実装 (構造体・共用体・可変長引数・コールバック: FEAT-0047) と bindgen (FEAT-0048) は **共通基盤** で、
aCHC と CirkaLang は宣言の構文だけが異なる。本書の §1〜§7 は aCHC の記法で書き、CirkaLang の記法は §8 に示す。
実用目標 (サーバ・syscall・Vulkan) に必要な要件は §9。

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

## 6. `import "C"` と bindgen

- `achc bindgen header.h [--allowlist 'sin|cos'] -o Math.chs` / `cirka bindgen header.h -o math.cirka` が、共通の `cirka-bindgen` (libclang) でヘッダを解析し `extern` ブロックを生成する。
- 既定の推測: ポインタ引数 → `Ptr`、`const T*` → `BorrowedPtr`、戻り値ポインタ → `Ptr`(所有権は推測しない)。
  所有権は利用者が生成ファイルを編集して与える(Rust の bindgen と同じ考え方)。
- `import "C" "math.h" (sin, cos)` はビルド時に bindgen を走らせてキャッシュする糖衣。

## 7. 実装

- 型検査: `extern` 宣言は通常の型シグネチャとして扱い、C ABI で表現可能な型か(`CInt` 等・ポインタ・構造体のみ)を検査する。
- MIR: `extern f(args)` 命令。LLVM では `declare` + `call ccc`。構造体の値渡し・可変長引数は x86_64 SysV ABI に従い展開する。
- `IO` 付きの呼び出しは `State#` トークンで順序付けられ、最適化で並べ替えられない。

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
- CirkaLang ではエラーは `Result` で返すラッパを標準とし、`errno` は `Result IOError a` に変換する (std.os、FEAT-0242)。
- 部分集合型を FFI 宣言に付けられる (例: `{n : CSize | n <= length buf}`)。宣言の述語は信頼される仮定として扱い、`--report` に列挙する (13 §1 P5)。

## 9. 実用目標のための FFI 要件

| 用途 | 必要な機能 | 担当 |
|------|------------|------|
| CUI / ファイル | `FILE*`・fd を RAII で扱う、`errno` → `Result` | FEAT-0213, 0242 |
| サーバ | ソケット fd、`epoll`、ノンブロッキング I/O、スレッドから安全に呼べること (`Send` 相当の区別) | FEAT-0046, 0240, 0241 |
| syscall | libc の syscall ラッパ、シグナルハンドラ (コールバック)、`mmap` | FEAT-0242 |
| Vulkan / GUI | 構造体・共用体の値渡しとアラインメント、ポインタの配列、ハンドル型 (64bit 不透明値) を `LType` として RAII 管理、関数ポインタの取得 (`vkGetInstanceProcAddr`)、コールバック (デバッグメッセンジャ)、大きなヘッダの bindgen | FEAT-0047, 0048, 0250, 0251 |
| 数値計算 | BLAS/LAPACK への配列の受け渡し (借用ポインタ、行優先/列優先) | FEAT-0252 |
