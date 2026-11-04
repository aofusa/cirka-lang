# 02. aCHC 言語仕様 (Haskell への拡張)

本書は aCHC が受け付ける言語を定義する。ベースは **Haskell 2010 Report**。本書は差分のみを記す。
型規則の詳細は [03-type-system.md](03-type-system.md)、実行時の意味は [04-memory-model.md](04-memory-model.md)。

## 1. ソースファイルと言語モード

| 拡張子 | モード | 既定で有効な拡張 |
|--------|--------|------------------|
| `.hs` | **Haskell モード** | なし (Haskell 2010 + `{-# LANGUAGE #-}` で明示した拡張) |
| `.chs` | **Cirka モード** | `CirkaOwnership`(下記)、`BangPatterns`、`LambdaCase`、`ScopedTypeVariables` |

- `.hs` に `{-# LANGUAGE CirkaOwnership #-}` を書けば `.chs` と同等になる。
- 両モードのモジュールは相互に import できる。`.hs` 側から見た Cirka 型(`Unique a` など)も通常の型として扱われ、
  型検査規則(線形専用の種など)は **定義側・使用側どちらのモードでも常に適用** される(健全性のため)。
- モードの違いは「新構文が使えるか」と「所有権に関する診断の既定レベル」だけで、**意味(評価結果)は変わらない**。

### 1.1 `CirkaOwnership` が有効にするもの

`LinearTypes` + 以下の Cirka 拡張:

| 拡張名 | 内容 | 節 |
|--------|------|----|
| `CirkaUnique` | `Unique` 型、種 `LType` | §3 |
| `CirkaDrop` | `Drop` / `Release` による自動解放 (RAII) | §4 |
| `CirkaBorrow` | `borrow` 構文糖 | §5 |
| `CirkaRegions` | `region` 構文糖 | §6 |
| `CirkaExtern` | `extern "C" { ... }` ブロック、`import "C"` | §7 |

個別に有効化することもできる。型・関数は `Cirka.*` モジュールに定義され、構文糖はそれらへの脱糖にすぎない。

### 1.2 サポートする GHC 拡張(第一段階の目標)

優先度順: `BangPatterns`, `ScopedTypeVariables`, `LambdaCase`, `TupleSections`, `MultiWayIf`,
`RankNTypes`, `ExistentialQuantification`, `MultiParamTypeClasses`, `FlexibleInstances`,
`FlexibleContexts`, `GeneralizedNewtypeDeriving`, `KindSignatures`, `LinearTypes`,
`Strict`, `StrictData`, `RecordWildCards`, `NamedFieldPuns`, `DeriveFunctor`/`Foldable`/`Traversable`,
`FunctionalDependencies`, `GADTs`, `TypeFamilies`, `DataKinds`(後半)。

サポートしない拡張を使うと、エラー `E0001 unsupported extension` を出す(黙って無視しない)。

## 2. 多重度 (Linear Haskell 互換)

GHC の `LinearTypes` と同じ構文・意味:

```haskell
f :: a %1 -> b         -- 線形関数: 引数をちょうど1回消費する
g :: a -> b            -- 通常の関数 (= a %Many -> b)
h :: a %m -> b         -- 多重度多相
e :: a %0 -> b         -- (Cirka 追加) 消去: 実行時に存在しない引数 (証明・型レベル用)

data Ur a where Ur :: a -> Ur a        -- 無制限値を線形文脈で運ぶ
```

- `%1` を `⊸` と書いてもよい(`UnicodeSyntax` 有効時)。
- `let`/`where` 束縛の多重度は GHC と同じく注釈 `let %1 x = ...` で指定できる。省略時は推論(GHC と異なり aCHC は推論する: [03-type-system.md](03-type-system.md) §2.3)。
- **`%0` は aCHC の追加**。`%0` の引数は実行時に消去され、値を実行時に使うことは型エラー。

## 3. 一意性型 (`CirkaUnique`)

### 3.1 種 `Type` と `LType`

```haskell
Type   -- 共有可能な型 (通常の Haskell の型)。値は何度でも使える
LType  -- 線形専用の型。値は多重度 1 (または 0) でしか束縛できない
-- 部分種関係: Type ⊑ LType (共有可能な値は線形にも使える)
```

`Cirka.Ownership` が以下を提供する:

```haskell
type Unique :: Type -> LType
-- 値 Unique x : x への唯一の参照。破壊的更新が安全にできる。表現は x と同一 (ゼロコスト)

unique   :: a -> Unique a                       -- 必要ならコピーして一意にする (RC==1 なら O(1))
freeze   :: Unique a %1 -> a                    -- 一意性を捨てて共有可能にする (O(1))
-- 例: 可変配列
newArray :: Int -> e -> Unique (Array e)
write    :: Unique (Array e) %1 -> Int -> e -> Unique (Array e)   -- in-place
read     :: Unique (Array e) %1 -> Int -> (Unique (Array e), Ur e)
```

### 3.2 利用者定義の線形専用型

```haskell
-- 種注釈で線形専用であることを宣言
type Socket :: LType
data Socket = Socket CInt

-- フィールドに LType を含むデータ型は自動的に LType になる (一意性の伝播)
data Conn = Conn Socket Buffer      -- Conn :: LType
```

型変数は既定で `Type`。線形専用型でも具体化できる多相関数を書くには種注釈が必要:

```haskell
swap :: forall (a :: LType) (b :: LType). (a, b) %1 -> (b, a)
swap (x, y) = (y, x)

dup :: a -> (a, a)      -- a :: Type なので Unique X では呼べない (型エラー)
```

組み込みのタプル・`Maybe`・`Either`・リストは **種多相** で、要素の種の上限が全体の種になる
(`Maybe (Unique X) :: LType`)。

## 4. RAII: `Drop` と `Release` (`CirkaDrop`)

```haskell
-- 純粋な解放: メモリやオフヒープ領域を返すだけで、観測可能な副作用を持たない
class Drop (a :: LType) where
  drop :: a %1 -> ()

-- 副作用を伴う解放: ファイルを閉じる、ロックを外す、フラッシュする等
class Release (a :: LType) where
  release :: a %1 -> IO ()
```

### 自動挿入の規則

- 線形専用型の束縛 `x` が、ある制御経路で消費されずにスコープを抜ける場合:
  - `Drop` インスタンスがあれば、コンパイラがその経路の終点に `drop x` を挿入する(純粋・IOどちらの文脈でも)。
  - `Release` インスタンスのみがある場合、`IO` の `do` ブロック内でのみ、ブロック末尾(および例外経路)に `release x` を挿入する。純粋な文脈ではエラー。
  - どちらもなければエラー `E0302 linear value not consumed`(Linear Haskell と同じ「必ず使え」)。
- 同一スコープ内の複数の値は **束縛の逆順** に解放する(Rust と同じ)。
- `Unique a`(`a :: Type`)は常に暗黙の `Drop` を持つ(メモリ解放)。
- `Drop` を持つ型は **アフィン型**(高々1回使用)として振る舞う。持たない型は **線形型**(ちょうど1回)。

```haskell
main :: IO ()
main = do
  h <- openFile "a.txt" ReadMode     -- h :: Handle_ (LType, Release)
  (h, line) <- hGetLine h
  putStrLn line
  -- ここで release h が自動挿入される (例外時も)
```

## 5. 借用 (`CirkaBorrow`)

ランク2型のスコープ変数 `r` でライフタイムを表す(`ST` と同じ原理)。借用を通じたアクセスはすべて
`r` で添字付けられた **借用モナド `Borrow r`** の中で行う。`r` は結果型 `b` に現れないので、借用も、
借用を捕捉したクロージャ(型に `Borrow r` が現れる)も脱出できない。

```haskell
type Ref    :: Region -> LType -> Type     -- 共有借用 (読み取り専用)
type MutRef :: Region -> LType -> Type     -- 排他借用 (書き込み可能)
type Borrow :: Region -> Type -> Type      -- 借用操作のモナド (ST r と同様、正格に逐次実行)

borrow    :: a %1 -> (forall r. Ref r a    -> Borrow r b) %1 -> (a, b)   -- b は r を含まない
borrowMut :: a %1 -> (forall r. MutRef r a -> Borrow r b) %1 -> (a, b)
-- 借用中に別の値を借用する (ネスト)
borrowIn  :: a %1 -> (forall r'. Ref r' a -> Borrow r' b) %1 -> Borrow r (a, b)

-- 借用越しの操作 (例: 配列)
length    :: Ref r (Array e) -> Borrow r Int
index     :: Ref r (Array e) -> Int -> Borrow r e         -- e :: Type (要素はRCで複製して返す)
writeAt   :: MutRef r (Array e) -> Int -> e -> Borrow r ()
asRef     :: MutRef r a -> Ref r a
```

**なぜモナドが必要か**: もし `index :: Ref r (Array e) -> Int -> e` のような純粋関数にすると、
`borrow arr (\a -> \i -> index a i)` が型 `Int -> e`(`r` を含まない)のクロージャとして借用を
持ち出せてしまう。`Borrow r` に閉じ込めることで、借用を使う計算の型には必ず `r` が現れる。

**健全性の要点**: `borrow`/`borrowMut` は所有者 `a` を線形に受け取り、コールバックの実行が
完了してから返す。したがって借用中に所有者が消費・解放されることはなく、`Ref` と `MutRef` が
同時に存在することもない。`Borrow r` の計算は `borrow` の呼び出し時に正格に実行される。

構文糖:

```haskell
borrow arr as a in do
  n <- length a
  sum <$> mapM (index a) [0 .. n - 1]
-- ⇒ borrow arr (\a -> do ...)   結果は (arr, 合計)
```

実行時には `Ref`/`MutRef` は単なるポインタ、`Borrow r` は `State# RealWorld` と同様に消去され、RC 操作は発生しない。

## 6. リージョン (`CirkaRegions`)

```haskell
type Region :: Type   -- 種。リージョン変数 r の種でもある

withRegion :: (forall r. Arena r -> RIO r a) -> IO a        -- a は r を含まない
allocIn    :: Arena r -> a -> RIO r (In r a)                -- アリーナ上に確保
readIn     :: In r a -> RIO r a                             -- 読み出しも RIO r の中だけ (借用と同じ理由)
newRefIn   :: Arena r -> a -> RIO r (RefIn r a)             -- リージョン内の可変参照 (循環可)
liftIO     :: IO a -> RIO r a
-- In r a は Type。アリーナが生きている間だけ有効 (r で脱出を防ぐ)。スコープ終了時に一括解放
```

リージョン内のオブジェクトへのポインタ (`In r a`) は RC の対象外。リージョン内オブジェクトが
通常のヒープ値を指す場合、その参照はリージョンが保持し(dup)、リージョン解放時に drop する。

構文糖: `region r in do ...`。循環を含むグラフ構造(knot-tying)は、リージョン内で作れば RC を使わず安全に回収できる。

## 7. C FFI (`CirkaExtern`)

Haskell 2010 の `foreign import ccall` はそのまま使える(`.hs` でも可)。Cirka モードでは加えて:

```haskell
extern "C" {
  type CFile                                        -- 不透明型
  fopen  :: CString -> CString -> IO (Nullable (OwnedPtr CFile))
  fclose :: OwnedPtr CFile %1 -> IO CInt
  strlen :: BorrowedPtr CChar -> CSize              -- IO なし = 純粋と宣言 (利用者の責任)
}

import "C" "math.h" (sin, cos)                     -- ヘッダから型を取得 (achc bindgen)
```

詳細は [08-ffi.md](08-ffi.md)。

## 8. 評価戦略に関する追加規則

- Haskell の遅延評価を既定とする(`.hs` も `.chs` も同じ)。`Strict` / `StrictData` で正格にできる。
- **線形専用型 (`LType`) の値は常に WHNF に評価済み** として扱う:
  - `LType` の引数・フィールドは正格。
  - 右辺が線形変数を含む `let` は **正格 let** として束縛時点で評価する(資源の解放順序を決定的にするため)。
  - 線形変数を捕捉するサンクは作らない。
- この規則は `CirkaOwnership` 由来の型を使うコードにしか影響せず、既存 Haskell コードの意味は変わらない。
  ただし GHC の `LinearTypes` を使った既存コードでは、遅延だった `let` が正格になる場合がある(互換性注記に記載)。

## 9. GHC との既知の非互換 (第一段階)

| 項目 | aCHC の挙動 |
|------|-------------|
| 循環データ (`let xs = 1 : xs`) | 動作するが参照カウントでは回収できないためリークする。コンパイル時に警告 `W0401`。`--cycles=collect` で回収器を有効化可能 |
| `System.Mem.performGC`, `System.Mem.Weak` のファイナライザ | `performGC` は no-op。Weak は RC ベースで、最後の強参照消滅時にファイナライザを実行 |
| 非同期例外 (`throwTo`, `timeout`) | 非対応 |
| Template Haskell / GHCi | 非対応 |
| `GHC.*` 内部モジュール・primop | 一部のみ (`GHC.Exts` の公開 API の一部) |
| `unsafePerformIO` | 対応。ただし `NOINLINE` グローバル `IORef` パターンは静的初期化で実装 |
| `LinearTypes` 使用コードの `let` | 線形変数を含む `let` は正格 (§8) |
| RTS オプション (`+RTS -N`) | 非対応。ランタイム設定は環境変数 (`CIRKA_*`) |
