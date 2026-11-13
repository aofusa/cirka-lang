# CirkaLang v0.0.3 言語仕様

- 版: v0.0.3 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0220 (借用)、FEAT-0221 (リージョン)、FEAT-0222 (`Lazy` と循環)、FEAT-0223 (リリース)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.3: 借用・リージョン・性能」を確定したもの。

v0.0.3 の目的は、**所有権を手で受け渡さずに、線形な値を一時的に貸して使える** ようにし、性能目標 G3 (Rust の 1.5 倍以内) に近づくことである。
この文書には [spec-v0.0.2.md](spec-v0.0.2.md) からの差分だけを書く。書いていないことは v0.0.2 のとおり。
各節は対応するチケットの完了時に書き足す。

## 1. 変更の一覧

| 節 | 内容 | チケット |
|----|------|----------|
| §2.1 | スコープ付き借用 `borrow` / `borrowIO`、リージョン `Region`、参照 `Ref a r`、借用のモナド `Borrow a r` / `BorrowIO a r` | FEAT-0220 |
| §2.2 | 借用の糖衣 `borrow x as a in e` (リージョンを書かない) | FEAT-0220 |
| §2.3 | 借用越しの操作: `Data.Array` / `Data.Array.Unboxed` の `lengthB` `readB` `writeB` `modifyB` `swapB`、`Std.Fs` の `readB` `readAllB` `writeB` | FEAT-0220 |
| §2.4 | `impl` の変数の種の宣言 `impl {a : LType} -> C (T a)` | FEAT-0220 |
| §2.5 | 実行時の表現: 借用は所有者を受け渡す状態の関数で、参照数の操作も確保もない | FEAT-0220 |
| §3 | リージョン `Data.Region`: `withRegion`、`Arena r`、`RIO r`、不変のハンドル `In r a`、可変の参照 `RefIn r a` | FEAT-0221 |
| §4.1 | 明示的な遅延 `Data.Lazy`: `Lazy a`、`delay`、`force`、`now`、`mapLazy` | FEAT-0222 |
| §4.2 | 循環の静的な扱い: `Lazy` を持つトップレベルの値の警告 W0310 | FEAT-0222 |
| §4.3 | `&&` と `\|\|` の短絡評価 | FEAT-0222 |

## 2. 借用 (FEAT-0220)

### 2.1 スコープ付き借用

`Data.Borrow` が次を定める。

```
Region : Type                                   -- 借用のリージョン (消去される)
Ref : LType -> Region -> Type                   -- リージョン r で a を借りている印 (消去される)
Borrow : LType -> Region -> Type -> Type        -- r の中で借りた値を読み書きする計算
BorrowIO : LType -> Region -> Type -> Type      -- 同じくアクションを含む計算

borrow   : {a : LType} -> (1 x : a) -> ((0 r : Region) -> Ref a r -> Borrow a r b) -> (a, b)
borrowIO : {a : LType} -> (1 x : a) -> ((0 r : Region) -> Ref a r -> BorrowIO a r b) -> IO1 (a, b)

using   : Ref a r -> ((1 s : a) -> (a, Ur c)) -> Borrow a r c       -- 値を受け取り、返しつつ結果を出す
update  : Ref a r -> ((1 s : a) -> a) -> Borrow a r ()              -- 値を置き換える
usingIO : Ref a r -> ((1 s : a) -> IO1 (a, Ur c)) -> BorrowIO a r c
liftIO  : IO c -> BorrowIO a r c                                     -- 値を使わないアクション
```

- `borrow x f` は線形な値 `x` を `f` に貸し、`f` の計算が終わったら `x` (更新されたもの) を結果と組にして返す。
  所有者の側から見ると `x` は消費され、同じ値が戻ってくる。Rust の `&mut x` を関数内で使い終えるのと同じである。
- `Borrow a r` と `BorrowIO a r` は `Functor` と `Monad` の実装を持ち、`do` で書ける。
  `Ref a r` は何度でも使える (`Type`) が、操作は計算の中でしか行えず、計算は `borrow` が走らせる。
- **持ち出しの禁止**: `r` は `f` の多重度 0 の引数であり、`borrow` の結果の型 `b` は `r` より外側で決まる。
  したがって `Ref a r` や `Borrow a r c` を結果にしようとすると、`b` に `r` を含む型を入れることになり型エラー (E1200) になる。
  この場合は「リージョン `r` は借用の中にしかない」という注記を付ける。

```
case borrow arr as a in pure a of ...           -- E1200: 参照を持ち出せない
case borrow arr as a in pure (readB a 0) of ... -- E1200: r の計算も持ち出せない
```

- `Ref` の区別は 1 種類である。設計 (12-cirkalang.md §4.4) の `Ref` / `MutRef` は、読むだけか更新するかを操作 (`using` / `update`) で区別するのでまとめた。
  共有可能な値は借用せずにそのまま何度でも使える (`Type`) ので、共有の借用 (`&T`) は要らない。

### 2.2 借用の糖衣

```
borrow x as a in e      ==  borrow x (\_, a => e)
borrowIO x as a in e    ==  borrowIO x (\_, a => e)
```

- `borrow` / `borrowIO` は予約語ではない。式の先頭の `borrow` の後に原子式と識別子 `as` が続くときだけ糖衣と読む (`x` は原子式なので、式は括弧で囲む)。
  それ以外は通常の関数呼び出しである。`e` は `in` の後ろ全体に伸びる (`let` と同じ)。
- **リージョンの省略**: 関数の中で完結する借用はリージョンを名前で書かなくてよい。`Ref a r` を受け取る補助関数を書くときだけ、シグネチャに `r` を書く
  (シグネチャの型変数として暗黙に束縛される)。
- Rust 風の `&x` / `&mut x` は採らない (ADR-071)。借用は所有者の変数の書き換えを伴うので、ブロックの範囲を明示する方が線形性の規則と合う。

### 2.3 借用越しの操作

| モジュール | 関数 | 型 (`T = Unique (Array e)` / `Unique (UArray e)`) |
|------------|------|------|
| `Data.Array`, `Data.Array.Unboxed` | `lengthB` | `Ref T r -> Borrow T r Int` |
| | `readB` | `Ref T r -> Int -> Borrow T r e` |
| | `writeB` | `Ref T r -> Int -> e -> Borrow T r ()` |
| | `modifyB` | `Ref T r -> Int -> (e -> e) -> Borrow T r ()` |
| | `swapB` | `Ref T r -> Int -> Int -> Borrow T r ()` |
| `Std.Fs` | `readB` | `Ref File r -> Int -> BorrowIO File r (Result IOError String)` |
| | `readAllB` | `Ref File r -> BorrowIO File r (Result IOError String)` |
| | `writeB` | `Ref File r -> String -> BorrowIO File r (Result IOError ())` |

`Data.Array.Unboxed` の版は `Unbox e =>` を取る。利用者は `using` / `update` / `usingIO` で同じ形の操作を定義できる。
共有可能な結果を `Ur` に入れる汎用の関数は、型変数の値が線形な値を隠しうる (spec-v0.0.2.md §5.2) ので用意しない。結果の型が具体的な操作ごとに書く。

```
import Data.Array.Unboxed as U
import Data.Borrow

sortStep : Ref (Unique (U.UArray Int)) r -> Int -> Borrow (Unique (U.UArray Int)) r ()
sortStep a j = do
  x <- U.readB a (j - 1)
  y <- U.readB a j
  if x <= y then pure () else U.swapB a (j - 1) j

main : IO ()
main = case borrow (U.fromListU [3, 1, 2]) as a in sortStep a 1 of
  (arr, done) => print (U.toList (freeze arr))
```

### 2.4 `impl` の変数の種

```
impl {a : LType} -> Monad (Borrow a r) where ...
```

- `impl` の頭の型変数は既定で `Type` を範囲とする (spec-v0.0.2.md §5.1)。`{a, b : K} ->` を `impl` の直後 (制約より前) に書くと、その変数の種を宣言できる。
  `LType` と宣言した変数は線形な型で具体化できる。宣言しない変数は従来どおり頭から集める。
- 線形な型を受け取るのはメソッドの検査で安全が確かめられた場合だけなので、既定は変えない。

### 2.5 実行時の表現とコスト

- `Region` と `Ref a r` は実行時の値を持たない (単位値に消える)。
- `Borrow a r b` は「借りた値を受け取り、(値, `Ur b`) を返す関数」の newtype、`BorrowIO a r b` は「値を受け取り `IO (値, Ur b)` を返す関数」の newtype である。
  借りた値は線形に受け渡されるだけで、**複製も参照数の操作もない**。一意な配列の更新はその場で行われる。
- 共通基盤の Core 最適化は、newtype に包まれた状態の関数を返す関数に、状態を引数に取るワーカーを作る (IO の状態トークンと同じ扱い、06-ir.md §1.6)。
  これにより `do` の bind が作るクロージャが消え、借用越しのループは所有者を手で受け渡すループと同じコードになる (tests/cirkalang.rs の `cirkalang_borrow_costs_nothing`)。
- 制限: 結果が `()` の借用の計算を末尾でない位置で呼ぶと、(値, `Ur ()`) の組が呼び出しごとに 1 つ確保される (構成した積の結果の最適化が、箱入りの値 1 つだけの結果を扱わないため。FEAT-0081)。

## 3. リージョン (FEAT-0221)

### 3.1 型と関数

`Data.Region` が次を定める。リージョンの添字は借用と同じ `Region` (`Data.Borrow`) である。

```
Arena : Region -> Type              -- リージョン r のアリーナ
RIO : Region -> Type -> Type        -- r の中のアクション (Functor / Monad)
In : Region -> Type -> Type         -- r に置いた不変の値のハンドル
RefIn : Region -> Type -> Type      -- r に置いた可変の参照

withRegion  : ((0 r : Region) -> Arena r -> RIO r a) -> IO a
allocIn     : Arena r -> a -> RIO r (In r a)
readIn      : In r a -> RIO r a
newRefIn    : Arena r -> a -> RIO r (RefIn r a)
readRefIn   : RefIn r a -> RIO r a
writeRefIn  : RefIn r a -> a -> RIO r ()
modifyRefIn : RefIn r a -> (a -> a) -> RIO r ()
liftRIO     : IO a -> RIO r a
```

- `withRegion f` は新しいリージョンを作って `f` を実行し、終わったらリージョンとその中の値をまとめて解放する
  (値は drop され、ほかでまだ使われている値は残る)。
- 置ける値は共有可能な型 (`Type`) に限る。線形な値はリージョンに置けない。
- **持ち出しの禁止**: 借用 (§2.1) と同じく `r` は `withRegion` が渡す多重度 0 の引数なので、ハンドルや `RIO r` の計算を結果にすると E1200
  (リージョンの注記付き) になる。
- ハンドルは参照数を持たないので、値どうしがハンドルを通して任意の形 (循環を含む) で参照し合っても参照数の循環にならず、
  循環の回収器なしにリージョンの終了で解放される。グラフはノードの型をリージョンで添字付けて書く:

```
import Data.Borrow (Region)
import Data.Region

data Node (r : Region) = MkNode Int (List (RefIn r (Node r)))

main : IO ()
main = do
  xs <- withRegion (\r, reg => do
    a <- newRefIn reg (MkNode 1 [])
    b <- newRefIn reg (MkNode 2 [a])
    writeRefIn a (MkNode 1 [b])         -- a と b が互いを指す
    walk a 5)
  print xs
```

### 3.2 実行時の表現

- 共通基盤のアリーナ (04-memory-model.md §6、FEAT-0049) をそのまま使う。`Arena r` はランタイムのリージョンのオブジェクト、
  ハンドルはセルのアドレス (`Int`)、`RIO r a` は `IO a` と同じ表現で、ラップと取り出しは消える。
- 読み出しは値の参照数を 1 増やし、書き込みは古い値を drop する。リージョンの終了時にセルの値をまとめて drop する。

## 4. 遅延と循環 (FEAT-0222)

### 4.1 明示的な遅延

`Data.Lazy` が次を定める。

```
Lazy : Type -> Type
delay   : (() -> a) -> Lazy a        -- 最初に force されたときに f () を計算する
force   : Lazy a -> a                -- 値 (未計算なら今計算し、結果を保持する)
now     : a -> Lazy a                -- 計算済みの値
mapLazy : (a -> b) -> Lazy a -> Lazy b
```

- 評価は正格のまま (spec-v0.0.1.md) で、遅延は `Lazy` で明示する (ADR-022)。`delay` は関数を受け取るので、引数の式は呼び出しの時点では評価されない。
- `Lazy a` は一度だけ計算する (メモ化)。同じ `Lazy` を何度 `force` しても計算は 1 回である。
- 遅延する値は共有可能な型 (`Type`) に限る (計算済みの値は共有される)。`Lazy` をフィールドに持つデータ型のパラメータは `(a : Type)` と書く。
- 実行時は共通基盤のサンク (遅延フィールドを 1 つ持つ箱) で、参照数で解放される。

```
import Data.Lazy

data Stream (a : Type) = Cons a (Lazy (Stream a))

from : Int -> Stream Int
from n = Cons n (delay (\_ => from (n + 1)))
```

### 4.2 循環

- 局所の `let` は再帰しないので、`Lazy` の値が自分自身を参照できるのは **トップレベルの値** (実行時の引数を取らない定義) を通してだけである。
  トップレベルの値は一度だけ評価されてプログラムの終わりまで保持される (spec-v0.0.2.md §5.3) ので、そこから `force` したものもすべて保持され続ける。
- 型が `Lazy` を含む (データ型のフィールドを通したものを含む) トップレベルの値は警告 **W0310** を出す。関数にする (`nats : () -> Stream Int`) か、
  循環するデータはリージョン (§3) で表す。
- 可変の参照はリージョンの `RefIn` だけで、参照数を持たない (§3.1) ので、参照数の循環を作らない。
  したがって v0.0.3 の CirkaLang では、トップレベルの値を除いて参照数の循環は作れない (循環の回収器は不要)。

### 4.3 短絡評価

- `a && b` は `if a then b else False`、`a || b` は `if a then True else b` として扱い、右辺は必要なときだけ評価する
  (spec-v0.0.1.md §5 の「両辺を評価する」を変更)。
- 演算子を 2 つの引数に適用したときだけである。関数として渡した `(&&)` は Prelude の関数のまま (両方の引数を評価する)。

## 9. 対象外

v0.0.3 で扱わないもの (各チケットで更新する)。

- `&x` / `&mut x` の借用構文と、関数をまたぐリージョンの推論 (ADR-071)。
- 借用中の値の一部 (フィールド) だけを借りること。構造体の分解で書く。
- 2 つの値を同時に借りる専用の構文。組を借りるか、`borrow` を入れ子にする。
- リージョンに線形な値を置くこと、リージョンのアリーナに値そのものを確保すること (セルは値への参照を持つ。ADR-044)。
- `RIO` の中から外側のリージョンのハンドルを使う入れ子のリージョン (リージョンの包含関係の型)。
- 遅延する線形な値 (`Lazy` に線形な値を入れること)、`lazy e` のような遅延の構文 (`delay (\_ => e)` と書く)。
