# CirkaLang v0.0.2 言語仕様

- 版: v0.0.2 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0210 (トレイト)、FEAT-0211〜0215 (実用 CUI のその他の機能とリリース)、BUG-0015 (線形な値の複製の防止)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.2: 実用 CUI」を確定したもの。

v0.0.2 の目的は、**CLI ツールがこの言語だけで書ける** ようにすることである。
この文書には [spec-v0.0.1.md](spec-v0.0.1.md) からの差分だけを書く。書いていないことは v0.0.1 のとおり。

## 1. 変更の一覧

| 節 | 内容 | チケット |
|----|------|----------|
| §2 | 字句: `trait` `impl` `deriving` が予約語 | FEAT-0210 |
| §2 | 字句: 単独の `?` は記号 | FEAT-0212 |
| §3 | トレイト、`impl`、導出、制約 `C a => T` | FEAT-0210 |
| §3.5 | `Drop` と `Release` の導出 | FEAT-0212 |
| §3.6 | 数値リテラルの型 (`fromInt`、`Int` への既定) | FEAT-0210 |
| §3.7 | `do` は `Monad` のメソッドに変換される。`IO` / `IO1` の `do` は文ごとに束縛を選ぶ | FEAT-0210, FEAT-0212 |
| §3.8 | Prelude の算術・比較はトレイトのメソッド | FEAT-0210 |
| §4 | パッケージ: `cirka.toml` のパス依存と名前空間 | FEAT-0214 |
| §5.1 | データ型のパラメータは線形性を伝播する。型変数とラムダの束縛子の宇宙の検査。資源を持たない値は複製できる | BUG-0015 |
| §5.2 | 線形な値を保持する値 (部分適用・アクション) は線形 | BUG-0015 |
| §5.3 | トップレベルの値は線形な値を持てない (アクションは実行ごとに作る) | BUG-0015 |
| §5.4 | `IO1`: 結果が線形なアクション | FEAT-0212 |
| §5.5 | `Drop` / `Release` と終了処理の自動挿入、リークの警告 | FEAT-0212 |
| §5.6 | `ω` と宣言したフィールドと `Ur` (線形な値から取り出しても何度でも使える値) | FEAT-0211 |
| §6 | `Result` と `?`、パニックの補助関数。`panic` の結果は線形な型でもよい | FEAT-0212, FEAT-0211 |
| §7.1 | `import` の別名 (`import A.B as X`) と名前の一覧 (`import M (a, b)`) | FEAT-0211 |
| §7.2 | `Data.Array` (箱入り配列、`Unique` で一意な更新) と `Data.Vec` (伸長する配列) | FEAT-0211 |
| §7.3 | `Data.Array.Unboxed` (箱なし配列) とトレイト `Unbox` | FEAT-0211 |
| §7.4 | `Data.String` (バイト列のビュー `Str`、分割・検索、`Builder`) | FEAT-0211 |
| §8.1 | `extern "C"` / `extern "cirka"` ブロック (文脈キーワード `extern` `link`) | FEAT-0213 |
| §8.2 | 境界を越えられる型、`Foreign` (C の型・`Ptr`・C 文字列) | FEAT-0213 |
| §8.3 | 標準ライブラリ `Std.IO` `Std.Fs` `Std.Env` `Std.Process` `Std.Time`。Prelude に `print` `isJust` `isNothing` | FEAT-0213 |

## 2. 字句

`trait` `impl` は v0.0.1 の「将来用の予約語」から通常の予約語になる。新たに `deriving` を予約する。
`total` `partial` `covering` `mutual` は引き続き将来用である (E1004)。

単独の `?` は演算子ではなく記号であり、`do` の文の末尾にだけ書ける (§6.2)。`?` という名前の演算子は定義できない (`?>` などは引き続き演算子)。

## 3. トレイト

### 3.1 構文

```ebnf
decl      ::= ... | trait | impl
trait     ::= "trait" [ context "=>" ] UIDENT { tparam } [ "where" "{" { titem ";" } "}" ]
impl      ::= "impl" [ context "=>" ] expr [ "where" "{" { clause ";" } "}" ]
context   ::= expr | "(" expr { "," expr } ")"
tparam    ::= LIDENT | "(" LIDENT ":" expr ")"
titem     ::= sig | clause                  -- メソッドのシグネチャと既定の定義
data      ::= ... [ "deriving" ( UIDENT | "(" [ UIDENT { "," UIDENT } ] ")" ) ]
expr      ::= ... | opexpr "=>" expr        -- 制約付きの型 (関数の型の中に書ける)
```

- `C a => T` は暗黙の引数 `{_ : C a} -> T` (多重度 ω) である。`(C a, D b) => T` は複数の制約を表す。
- `trait` の型パラメータの既定の種は `Type`。`trait Functor (f : Type -> Type)` のように注釈すれば高階にできる。
- `impl` のパラメータは先頭に自動で束縛する (`impl Show a => Show (List a)` の `a`)。

```
trait Eq a where
  (==), (/=) : a -> a -> Bool
  x /= y = not (x == y)

impl Eq Color where
  Red == Red = True
  _ == _ = False

impl Show a => Show (List a) where
  show xs = ...
```

### 3.2 トレイトの要素

- **メソッド** はトップレベルの関数として見える。`(==) : Eq a => a -> a -> Bool` のように、最初の暗黙引数がトレイトの辞書である。
- **既定の定義** はメソッドの本体で、`impl` で定義されなければ使われる。既定の定義のないメソッドを `impl` が定義しなければ E1255。
- **スーパートレイト** は宣言の文脈に書く (`trait Eq a => Ord a`)。`Ord a` があれば `Eq a` も使える。循環は E1257。
- `impl` を書くとき、スーパートレイトの `impl` も存在しなければならない (E1253)。

### 3.3 解決

- `C t` の制約は型検査の最後まで保留でき、単一化の結果を待って解決する。局所の制約 (関数の引数の辞書とそのスーパートレイト) を先に調べ、次に `impl` の頭部と一致を取る。
- `impl` の頭部は **パターン** である。型変数の解決はこの一致だけで決まり、文脈は頭部が一致したあとの副目標となる。頭部に現れない型変数を文脈に書くと E1256。
- 解決できなければ E1253、型が決まらず候補が確定できなければ E1254。

### 3.4 一貫性 (孤児実装の禁止)

- `impl` は **トレイトか、頭部に現れる型の定義モジュール** にのみ書ける (E1251)。
- 同じトレイトに対して頭部が重なる `impl` は書けない (E1252)。重なりの検査では型変数は何にでも一致するとみなす。

### 3.5 導出

`deriving` は `Eq` `Ord` `Show` `Drop` `Release` を構文として展開する (他は E1116)。各パラメータ `a` について `C a` を要求する `impl` を生成する。
`Ord` は構築子の宣言順、フィールドの辞書式順で比較する。`Show` は Haskell の `show` と同じく括弧の深さを `showPrec` で扱う。
`Drop` / `Release` は実行時のフィールドを宣言の順に終了する (§5.5)。線形なデータ型にだけ導出できる (E1258)。

### 3.6 数値リテラル

- 整数リテラルの型は文脈で決まる。`Int` `UInt` `Nat` `Float` なら直接その値になり、他の型なら `Num` の `fromInt` を呼ぶ (`Num` の `impl` がなければ E1200)。
- 型が決まらないまま残ったリテラルは `Int` になる。
- 型検査は数値の演算子のオペランドから型を推論する。`2 * 1.5` は `Float` になる。

### 3.7 `do` 記法

`do` は `Monad` の `(>>=)` に変換する (`e; rest` は `e >>= \_ => rest`。正格評価なので `rest` をラムダの下に置く)。
ただし期待される型の頭部が `IO` か `IO1` なら、各文のアクションの型から `primBind` などを直接選ぶ (§5.4。継続が 1 回だけ実行されることを型で表すため。線形な値を捕獲できる)。

推論する `case` / `if` の結果の型は、対象の変数に依存しない型とする (`do` の文として `if c then use r else pure ()` と書ける)。

### 3.8 標準トレイト

| トレイト | メソッド | スーパートレイト |
|----------|----------|------------------|
| `Eq a` | `(==)` `(/=)` | |
| `Ord a` | `compare` `(<)` `(<=)` `(>)` `(>=)` | `Eq a` |
| `Show a` | `show` `showPrec` | |
| `Num a` | `(+)` `(-)` `(*)` `negate` `fromInt` | |
| `Div a` | `(/)` `(%)` | |
| `Functor f` | `fmap` | |
| `Monad m` | `pure` `(>>=)` | `Functor m` |

`Int` `UInt` `Float` `Char` `String` `Unit` `Bool` `Ordering` `Pair` `List` `Maybe` `Either` `IO` に `impl` がある (該当するもののみ)。
`(>>)` は `Monad` を要求する通常の関数である。`min` `max` `abs` `sum` `product` `mapM_` はトレイトに対して多相である。

v0.0.1 の `Float` 専用演算子 (`+.` など) は残す。

### 3.9 実行時の表現

- トレイトはデータ型 `C` と構築子 `C$Dict` に変換される。フィールドはスーパートレイトの辞書、次に各メソッドの順で、多重度 0 のものは含まれない。
- `impl` はグローバルの辞書 (文脈があれば辞書を受け取る関数) である。
- 既知の型で使うかぎり、**実行時に辞書は残らない**。Core の特殊化が文脈の辞書を順に定数にし、メソッドの選択は既知の構築子として簡約される (`docs/design/16-performance.md`)。
- `IO` は Core の newtype (`Unit# -> a`) である。

### 3.10 診断

| コード | 内容 |
|--------|------|
| E1006 | `trait` / `impl` の中に書けない宣言 |
| E1115 | トレイトにないメソッド、`impl` の頭部がトレイトでない |
| E1116 | 導出できない |
| E1250 | 制約がトレイトでない |
| E1251 | 孤児実装 |
| E1252 | 重なる `impl` |
| E1253 | インスタンスがない |
| E1254 | 型が決まらずインスタンスを選べない |
| E1255 | 定義がなく既定もないメソッド |
| E1256 | 頭部にない型変数 |
| E1257 | スーパートレイトの循環 |

## 4. パッケージ

### 4.1 `cirka.toml`

```toml
[package]
name = "app"            # パッケージ名 (英小文字・数字・`-`・`_`)
version = "0.1.0"       # 省略可 (まだ解釈しない)
edition = "0.0.2"       # 対象とする版 (コンパイラの版と一致しなければならない)

[dependencies]
util = { path = "../util" }        # パス依存 (このファイルからの相対パス)
```

- 受け付ける TOML は上の部分集合 (節、`key = "文字列"`、`key = { path = "文字列" }`、コメント) だけである。誤りは E1001。
- ソースは `src/` に置き、プログラムの `main` は `src/Main.cirka` のモジュール `Main` である。
- `cirka build|run|check|test` にディレクトリか `cirka.toml` を渡すと、パッケージとしてビルドする。
- `src/Main.cirka` のないパッケージは **ライブラリ** である。`cirka check` / `cirka test` は `src/` のすべてのモジュールを対象にし、
  `cirka build` / `cirka run` は E1107。
- `cirka check` は `main` のないプログラムも受け付ける (型・所有権の検査と Core Lint まで行う)。

### 4.2 名前空間

- 各パッケージは **名前空間** を持つ。パッケージ名をパスカルケースにしたもの (`util` → `Util`、`math-tools` → `MathTools`)。
- 依存されるパッケージのモジュールは、その名前空間の下に置く: `src/Util.cirka` がモジュール `Util`、`src/Util/Strings.cirka` が `Util.Strings`。
  名前空間の外のモジュールは他のパッケージから見えない (`import` しても見つからない)。
- 依存元 (ビルドするパッケージ) のモジュール名は自由である。ただし依存の名前空間と同じ先頭を持つ名前はその依存のものになる。
- `import` できるのは、自分のパッケージのモジュール、`[dependencies]` に書いたパッケージのモジュール、コアライブラリ (`Prelude` など) だけである。
  依存の依存を直接 `import` すると E1119 (推移的な依存は自動では見えない)。

### 4.3 依存の解決

- 依存は推移的にたどる。同じディレクトリを指す依存は一つのパッケージとして扱う (菱形の依存)。
- 依存の循環、`[dependencies]` に書いた名前と依存先の `name` の不一致、同じ名前の別のパッケージは E1118。
- ビルドは全体プログラムで行う (パッケージごとの分割コンパイルはしない。01-architecture.md §6)。

| コード | 内容 |
|--------|------|
| E1118 | 依存の誤り (循環・名前の不一致・同名の別パッケージ・名前空間にできない名前) |
| E1119 | 依存していないパッケージのモジュールを `import` した |

## 5. 所有権と資源

v0.0.1 の所有権 (spec-v0.0.1.md §4.2〜4.4) に、型パラメータを通した線形性、資源の終了処理 (RAII)、結果が線形なアクションを加える。
あわせて、線形な値を複製できてしまう穴 (BUG-0015) を塞ぐ。線形な値を 2 回使えると、利用者定義の `Release` で同じ資源を 2 回閉じられてしまうためである。

### 5.1 宇宙と型パラメータ

- データ型のパラメータに型を書かなければ (`data Box a = MkBox a`)、パラメータは `LType` を範囲とし、データ型はその線形性を **伝播** する。
  `Box Int` は共有可能 (`Type`)、`Box File` は線形である。`(a : LType)` と書いたパラメータも同じ。`(a : Type)` と書けば共有可能な型に限る。
  Prelude の `Pair` `List` `Maybe` `Either` `Result` もこの規則に従う (`List File` は線形)。
- 線形な型: `LType` と宣言したデータ型、伝播するパラメータに線形な型を渡したデータ型、`Unique a`、種が `LType` の型変数。
  型変数の適用 `f t` は、`f` の種が `LType` で終わるか、`LType` の位置の引数が線形なら線形である。
- トレイトのパラメータは既定で `Type` である (`trait Drop (a : LType)` と書けば線形な型も受け付ける)。
- 型変数を線形な型で具体化できるのは `{a : LType}` と宣言したときだけである。`{a : Type}` (型を書かない型変数の既定) を線形な型で具体化すると E0301。
- 型を書かないラムダの束縛子 (`\x => e`) は多重度 ω である。単一化の結果その型が線形になれば E0301 (`\(1 x : A) => e` と書く)。
  U-Bind (spec-v0.0.1.md §4.3) の検査は、メタ変数が解けた後にもう一度行う。
- **資源を持たない型** (spec-v0.0.1.md §4.4) の線形な束縛子 (`(1 n : Int)`、線形な値を分解したフィールドなど) は何回使ってもよく、ω で使われるクロージャにも捕捉できる。
  その値は線形な資源を含みえず、複製しても観測できないからである (v0.0.1 の「資源を持たない型の引数は 1 倍」と同じ考え方)。

### 5.2 線形な値を保持する値

部分適用・アクション・クロージャは、作るときに渡された値を保持する。`use : (1 t : Tok) -> Int -> Int` なら `use (MkTok 3)` は `Tok` を保持する関数であり、
2 回呼ぶと同じ `Tok` を 2 回使ってしまう。型には現れないので、次の規則で **保持者** を線形に扱う。

- 実行時の引数に、変数でない線形な型の式 (`MkTok 3` など) を渡す適用で、結果の型が線形な値を隠しうるもの (資源を持たない型を除く `Type` の型: 関数・`IO`・型変数など) は保持者である。
  変数を渡すときは v0.0.1 の規則どおり、その変数の使用として数える。線形な束縛子を使った `let` の値と、線形な対象の `case` の結果も、同じく保持者になりうる。
- 保持者を値とする `let` の束縛子と `case` の対象は多重度 1 になる。2 回使うと E0303 (保持している値の位置を示す)。
- 保持者を ω の位置 (ω の引数、ω で使われる関数の本体) に置くと E0305。
- 実行時のラムダの本体と `IO` の `do` の継続で作る値は、呼び出し・実行のたびに作られるので、外側の保持者にはならない。

```
main = let g = use (MkTok 3) in printInt (g 1 + g 2)     -- E0303: g は 1 回しか使えない
main = twice (useIO (MkTok 3))                           -- E0305: twice の引数は ω
```

### 5.3 トップレベルの値

トップレベルの値 (実行時の引数を取らない定義と、関数でないメソッド) は一度だけ評価され、すべての使用で共有される。

- 型が線形なら E0301 (`tok : () -> Tok` のような関数にする)。トレイトのメソッドの型も同じ。
- 評価が線形な値を保持するとき (§5.2)、型が `IO a` / `IO1 a` なら評価を実行時まで遅らせ、**実行のたびに** 線形な値を作る
  (`act = useIO (MkTok 1)` は `act = primBind (primPure ()) (\_ => useIO (MkTok 1))` として扱う)。それ以外の型なら E0305 (パラメータを書いて関数にする)。

### 5.4 結果が線形なアクション `IO1`

`IO a` の結果は多重度 ω で束縛されるので、`a` は共有可能な型に限る。ファイルなどの資源を返すアクションは `IO1` を使う。

```
prim IO1 : LType -> Type          -- アクションそのものは共有可能。実行のたびに新しい値を返す
pure1 : {a : LType} -> (1 x : a) -> IO1 a
lift1 : IO a -> IO1 a
```

`do` ブロックの型が `IO b` か `IO1 b` なら、各文のアクションを先に推論し、その型とブロックの型から束縛を選ぶ。
`IO1 a` の結果は多重度 1 で束縛される (`IO a` の結果は ω)。型の分からないアクションは `IO a` とする。

| 文のアクション | ブロック | 束縛 |
|----------------|----------|------|
| `IO a` | `IO b` | `primBind : (1 m : IO a) -> (1 k : a -> IO b) -> IO b` |
| `IO1 a` | `IO b` | `primBind1 : (1 m : IO1 a) -> (1 k : (1 x : a) -> IO b) -> IO b` |
| `IO a` | `IO1 b` | `primBindTo1 : (1 m : IO a) -> (1 k : a -> IO1 b) -> IO1 b` |
| `IO1 a` | `IO1 b` | `primBind11 : (1 m : IO1 a) -> (1 k : (1 x : a) -> IO1 b) -> IO1 b` |

アクション `m` も継続 `k` も 1 回だけ実行されるので、線形な値を保持・捕獲できる。実行時の表現は `IO` と同じ (`Unit# -> a`) である。

### 5.5 終了処理 (`Drop` / `Release`)

```
trait Drop (a : LType) where
  drop : (1 x : a) -> ()            -- 純粋な終了処理
trait Release (a : LType) where
  release : (1 x : a) -> IO ()      -- 入出力を伴う終了処理 (ファイルを閉じるなど)
```

- `impl` は線形な型にだけ書ける (E1258)。共有可能な値は参照カウントで解放され、終了処理を持たない。
- **終了処理を持つ型**: `Drop` か `Release` の (文脈のない) `impl` を持つデータ型、それを含む線形なデータ型、種が `LType` の型変数。
- メモリだけを持つ型の `Drop t` / `Release t` は常に解決でき、何もしない (`dropFree` / `releaseFree`)。`Drop t` があれば `Release t` も解決できる (`releaseByDrop`)。
  Prelude の `List` `Maybe` `Pair` `Either` `Result` は要素の `Drop` / `Release` から得られる。

**弱化.** 線形な値を使わずに捨てること (spec-v0.0.1.md §4.4 の Weaken-Drop) は、型によって次のように扱う。

| 型 | 弱化 |
|----|------|
| メモリだけを持つ型 | v0.0.1 と同じく何もしない (Perceus が解放する) |
| 終了処理を持つ型 | `Drop t` か `Release t` が解決できれば、コンパイラが終了処理を挿入する。できなければ E0302 / E0306 (導出を案内する) |
| コンストラクタのない `LType` の型 | できない (v0.0.1 と同じ) |

**挿入位置.** 線形な束縛子のスコープを分岐 (`case` / `if`) の枝ごとの **パス** に分け、束縛子を使わないパスに終了処理を入れる。

- スコープの値がアクション (`IO r` / `IO1 r`) なら、パスの **終わり** (アクションの最後) に入れる
  (`dropAfter x act` / `releaseAfter x act`。`IO1` は `dropAfter1` / `releaseAfter1`)。`Drop` があれば `Drop` を使う。
  一つのアクションで複数の値を捨てるときは、後に束縛したものから終了する (Rust の変数の解放順と同じ)。
- それ以外 (純粋な式) では、パスの **先頭** で `drop` を呼ぶ (束縛した順)。`Release` しか持たない値を純粋な式で捨てると E0307。
- 関数のパラメータは、実行時のパラメータをすべて受け取った後で終了する (部分適用では終了しない)。
- `case` で分解した値はその枝で使ったものとみなし、その枝で使わない線形なフィールドを終了する。
- `?` (§6.2) で早く終わるパスも同じ規則で終了する。

```
twoUnused : IO ()
twoUnused = do
  a <- open "a"                 -- open : String -> IO1 Res
  b <- open "b"
  putStrLn "body"               -- body を出力したあと release b、release a の順に実行する

maybeUse : Bool -> IO ()
maybeUse c = do
  r <- open "r"
  if c then use r else putStrLn "not used"     -- else の枝の終わりで release r
  putStrLn "after"
```

挿入は型検査とメタ変数の解決が終わった定義の本体に対して行い、結果は Core Lint を通る。

**リークの警告.** 終了処理を持つ値を保持する値 (§5.2) を `let` で束縛して使わないと、W0308 で警告する
(保持者は関数やアクションなので Perceus がメモリを解放するが、保持していた値の終了処理は実行されない)。
保持者を引数として渡した先で捨てられる場合は検出できない (リークは安全性の違反ではない。AGENTS.md の原則 3)。
`panic` で終わるときも終了処理は実行されない (§6.3)。

### 5.6 `ω` のフィールドと `Ur`

構成子のシグネチャで `(ω x : A)` と宣言したフィールドは **無制限** である。
線形な値を `case` で分解しても、そのフィールドに束縛した変数は何回使ってもよい。

```
data Ur (a : Type) : Type where          -- Prelude
  MkUr : (ω x : a) -> Ur a

case A.read u i of
  (u, MkUr x) => A.write (A.write u i x) (i + 1) x    -- x は 2 回使える
```

- `ω` のフィールドに置く値は ω の位置に置かれる (spec-v0.0.1.md §4.3 の U-App)。線形な変数を捕捉したクロージャを置くと E0305。
- `Ur` は「線形な値と一緒に返すが、それ自体は共有してよい値」を表す。配列の要素の読み出し (§7.2) が `(Unique (Array a), Ur a)` を返すのはこのためである。
  `(u, x)` だけでは、組が線形 (§5.1) なので `x` も 1 回しか使えない。
- `ω` は明示的なフィールドにだけ書ける。暗黙のフィールドは従来どおり多重度 0 である。

### 5.7 診断

| コード | 内容 |
|--------|------|
| E0301 | (追加) `Type` の型変数を線形な型で具体化した。型を書かないラムダの束縛子の型が線形になった。トップレベルの値・関数でないメソッドの型が線形 |
| E0302 / E0306 | (追加) 終了処理を持つ値を含むが、その型に `Drop` / `Release` がない (導出を案内する) |
| E0303 | (追加) 線形な値を保持する束縛子を 2 回以上使った |
| E0305 | (追加) 線形な値を保持する値を ω の位置に置いた、またはトップレベルの値が保持した |
| E0307 | `Release` しか持たない値を純粋な式で捨てた |
| E1258 | 共有可能な型に `Drop` / `Release` を実装・導出した |
| W0308 | 終了処理を持つ値を保持する束縛子を使わなかった (リーク) |

## 6. エラー処理

### 6.1 `Result`

```
data Result e a = Err e | Ok a deriving (Eq, Ord, Show)
```

- 例外はない。失敗しうる操作は `Result` (や `Maybe`) を返す。
- `Functor (Result e)` と `Monad (Result e)` がある。`Result` の `do` では `<-` が `Err` をそのまま返す。
- 補助関数: `isOk` `isErr` `mapErr` `okOr` (`Maybe` から) `unwrapOr`、パニックする `unwrap` `expect` (`Show e` を要求) と `expectJust`。

### 6.2 `?`

`IO` / `IO1` の `do` の文で、`Result` を返すアクションか `Result` の値の後ろに `?` を書くと、`Err` のときブロックを終える。

```
addAll : String -> String -> IO (Result ParseError Int)
addAll a b = do
  x <- parseNum a?            -- parseNum : String -> Result ParseError Int
  y <- readNum b?             -- readNum : String -> IO (Result ParseError Int)
  pure (Ok (x + y))
```

- `x <- e?` は、`e` (アクションなら実行した結果) が `Ok v` なら `x` を `v` に束縛して続け、`Err err` ならブロックの結果を `pure (Err err)` (`IO1` では `pure1 (Err err)`) とする。
  `e?` だけの文は `Ok` の値を捨てる。
- ブロックの結果の型は `Result` でなければならない (E1260)。誤りの型は変換しない (`e` の誤りの型と一致しなければ E1200。`mapErr` で合わせる)。
- `IO` / `IO1` 以外のモナドの `do` には書けない (E1260)。それらの `do` では `<-` が同じ働きをする。

### 6.3 パニック

`panic : {a : LType} -> String -> a` は v0.0.1 と同じく abort する (終了コード 1)。終了処理は実行されない。
戻らないので、結果の型は線形な型でもよい (線形な値を返す関数の失敗の枝に書ける)。
アンワインドして `Drop` / `Release` を実行するのは共通基盤の FEAT-0040 以降である。

### 6.4 診断

| コード | 内容 |
|--------|------|
| E1260 | `?` の誤り (`Result` でない値・ブロックが `Result` を返さない・`IO` / `IO1` 以外の `do`) |

## 7. モジュールと標準ライブラリ

### 7.1 `import` の別名と名前の一覧

```
import Data.Array as A               -- A.read, A.write, ... (修飾なしでは見えない)
import Data.String as S (Str, toStr) -- S.lines に加え、Str と toStr は修飾なしで使える
import Lib (x, (<+>))                -- x と <+> だけを修飾なしで使う。Lib.y は修飾すれば使える
```

- `import M as X` は `M` の公開名を `X.n` としてだけ見せる。`M.n` とも、修飾なしの `n` とも書けない。
  標準ライブラリの同名の関数 (`length` `map` `read` …) を並べて使うための形である。
- 別名は 1 つのモジュールにだけ付けられる。別の `import` で修飾なしに取り込んだモジュールの名前や、自身のモジュールの名前を別名にはできない (E1120)。
- `import M (a, b, (op))` は、一覧の名前だけを修飾なしで見せる。`M.n` の修飾は従来どおりすべての公開名に使える。
  一覧に存在しない名前を書くと E1101、`private` の名前を書くと E1106。
- 別名と一覧を両方書くと、一覧の名前は修飾なしで、すべての公開名は別名で使える。
- `import Prelude (…)` と明示すると、暗黙の Prelude もその一覧に限られる (別名を付けると Prelude は修飾が必要になる)。

### 7.2 箱入り配列 `Data.Array` と `Data.Vec`

```
prim Array : Type -> Type           -- 共有可能で読み出し専用
Unique (Array a)                    -- 一意な参照。更新はその場で行う
```

- 要素の型は共有可能な型 (`Type`) に限る。`Array a` は `a` が資源を持たなければ資源を持たない (§5.1)。
- `Unique (Array a)` の操作は、配列を受け取って返す: `new` `empty` `read : (1 u : Unique (Array a)) -> Int -> (Unique (Array a), Ur a)` `write` `length` `modify` `swap` `fromListU`。
  `freeze` で共有可能な `Array a` に、`thaw` で一意な配列に戻す (共有されていれば複製する)。
- `Array a` の操作: `size` `(!)` `get` (範囲外は `Nothing`) `fromList` `toList` `generate` `map` `foldl` `foldr`、`Eq` と `Show`。
  `Array` は Core では常に完全に適用されるので `Functor Array` はない (`map` を使う)。
- 範囲外の添字はパニックする (メッセージに関数名を含む。終了コード 1)。
- **`Unique` は速さの約束であって安全性の根拠ではない。** 更新は実行時に参照数が 1 であることを確かめ、共有されていれば先に複製する
  (共通基盤の `array_unique` / `array_set`)。そのため `Unique` の値を保持する部分適用・アクションは保持者 (§5.2) として扱わない。
  一意な配列への書き込みの連鎖が複製しないことは実行テスト (`array_unique`、配列の複製 0 回) で確かめる。

`Data.Vec` の `Vec a` は線形な伸長配列 (`MkVec (Unique (Array a)) Int`) である。
`empty` `replicate` `length` `push` (満杯なら容量を 2 倍 + 4 にする。償却定数時間) `pop : (1 v : Vec a) -> (Vec a, Ur (Maybe a))` `read` `write` `modify` `swap` `fromList` `toArray` `toList`。
`pop` で空いた枠の要素は、上書きされるか `Vec` が捨てられるまで生きている。

### 7.3 箱なし配列 `Data.Array.Unboxed`

`UArray a` は要素を箱なしの 8 バイトの語として持つ。要素の型はトレイト `Unbox` の実装を持つ `Int` `UInt` `Float` `Char` `Bool` である。

- 関数は `Data.Array` と同じ名前と形を持ち、`Unbox a =>` の制約が付く。`zeros n` は 0 の語で埋めた配列 (`0` `0.0` `'\0'` `False`)。
- `UArray a` は常に資源を持たない。
- `Unbox` の辞書は全体特殊化 (§3.9) で消え、ループは語を直接読み書きする。整数の配列のクイックソートは Rust と同等の速さで、ループの中では確保しない (`bench/array-sort`、実行テスト `uarray_noalloc`)。

### 7.4 文字列 `Data.String`

`String` は不変の UTF-8 テキストのままとし、部分文字列を複製せずに扱う **ビュー** と、その場で書き込む **ビルダ** を加える。添字と長さはバイト単位である。

```
data Str = MkStr String Int Int      -- s のバイト [i, j)
data Builder : LType where
  MkBuilder : (1 buf : Unique String) -> Int -> Int -> Builder
```

- 変換: `toStr` (全体のビュー)、`fromStr` (ビューの範囲を複製した文字列。全体ならそのまま)。
- ビューの操作 (複製しない): `length` `isEmpty` `byteAt` `slice` `take` `drop` `trim` `trimStart` `trimEnd` `splitOn` `lines` (`\r\n` の `\r` も除く) `words`、
  リストを作らない `foldLines` `foldWords`。
- 検索: `indexOf` `indexOfChar` `contains` `startsWith` `endsWith`。比較はバイト単位 (`Eq` / `Ord Str`。`Show Str` は文字列として表示)。
- 変換: `toInt : Str -> Maybe Int` (`-` / `+` の付いてよい 10 進)、`chars` (UTF-8 の復号) `fromChars` `toUpper` `toLower` (ASCII のみ)。
- 連結: `concat` `concatStr` `intercalate` `unlines` `unwords` `replicate` は結果の長さを先に数え、確保 1 回で書く。
- `Builder` は線形な値で、`builder cap` `append` `appendStr` `appendByte` `appendChar` `appendInt` で一意なバッファにその場で書き、満杯なら容量を 2 倍にする。`build` で `String` にする。
- 出力: `putStr` `putStrLn` (ビューの範囲を `fromStr` で文字列にして書く)。
- 低水準の操作 (`String` の範囲の複製・バイトの検索・範囲の比較・一意な文字列への書き込み) はモジュール内の `private prim` であり、利用者には見せない。

`bench/string-split` (100 万行を行・単語に分けて数え、`Builder` で出力する) は Rust の約 1.6 倍の時間である (確保は 1 行あたり 2 回)。残りの差は結合点 (join point) がないことによる箱の再構築で、共通基盤の FEAT-0080 で扱う。

## 8. C FFI と標準ライブラリ

### 8.1 `extern` ブロック

```
extern "C" where
  labs : Int -> Int
  getpid : IO CInt
  strlen : Ptr CChar -> IO CSize

private extern "cirka" where            -- 標準ライブラリ用 (ランタイムの規約)
  cirka_fd_write : Int -> String -> IO Int
```

- `extern "規約" [link "ライブラリ"] where` に続くブロックに `名前 : 型` を並べる。名前がそのまま関数の記号名になる。
  `extern` と `link` は文脈キーワードで、`extern` の直後に文字列が来たときだけ働く。`private` を付けるとブロックのすべての名前が非公開になる。
- 規約は `"C"` (プラットフォームの C ABI。x86_64 System V) と `"cirka"` (ランタイムの規約。`String` をランタイムの文字列オブジェクトとして借用で渡す) だけ (それ以外は E1007)。
  `"cirka"` は標準ライブラリがランタイムの関数を呼ぶためのもので、利用者のプログラムは `"C"` を使う。
- `link` に書けるのは、プログラムが常にリンクする `"c"` `"m"` `"pthread"` だけ (それ以外は E1007)。他のライブラリのリンクは FEAT-0047 で扱う。
- 結果が `IO t` (`IO1 t`) の関数はアクションで、実行するたびに呼ばれる。`IO` のない関数は **純粋と宣言したもの** として扱われ、評価の順序や回数は保証されない (宣言者の責任)。
- 同じ記号を、モジュールをまたいで (またはコンパイラ自身の使い方と) 異なる値の渡し方で宣言すると E1271。

### 8.2 境界を越えられる型

引数と結果は **1 つのスカラー** として渡される。

| 型 | C の型 (`"C"`) |
|----|----------------|
| `Int` / `UInt` / `Float` / `Char` / `Bool` | `int64_t` / `uint64_t` / `double` / `uint32_t` / `int` (偽は 0) |
| `Foreign` の C の型 | `CChar` `CUChar` `CShort` `CUShort` `CInt` `CUInt` `CLong` `CULong` `CSize` `CSSize` `CFloat` `CDouble` は名前どおり |
| `Ptr a` / `OwnedPtr a` | `void *` |
| 構成子が 1 つで実行時のフィールドが 1 つのデータ型 (`data Pid = MkPid CInt`) | フィールドの型 |
| `String` | `"cirka"` 規約のみ (ランタイムの文字列オブジェクト) |
| `()` (結果のみ) | `void` |

- それ以外の型 (リスト、フィールドが 2 つ以上のデータ型、関数など) は E1270。構造体の値渡し・コールバック・可変長引数は FEAT-0047。
- C の型は `Int` / `Float` を包むデータ型である (`MkCInt 3`、`fromCInt`)。C へ渡すときに下位のビットに切り詰め、C からの結果は 64 ビットに符号拡張 (符号なしの型はゼロ拡張) する。
- `OwnedPtr a` は線形 (`LType`) で、解放の責任を表す。解放する関数は型から分からないので、`Release` を持つ利用者の型で包む (08-ffi.md §8)。

`Foreign` (core) の関数: `nullPtr` `isNull` `plusPtr` `castPtr` `ptrAddress`、`mallocBytes` / `freeBytes`、
`withCString : String -> (Ptr CChar -> IO b) -> IO b` (NUL 終端の複製を渡し、後で解放する)、`peekCString` `peekCStringLen`、
`peekByte` `pokeByte` `peekInt32` `pokeInt32` `peekInt64` `pokeInt64` (添字は要素単位)。ポインタを通した読み書きは検査しない (C と同じ)。

### 8.3 標準ライブラリ (`lib/cirkalang/std`)

失敗しうる操作は `Result IOError a` を返し、プログラムを終わらせない。`IOError` は OS のエラー番号 (`errno`) を持つ。
ライブラリの探索パスは `core` と `std` の 2 つである。

| モジュール | 内容 |
|------------|------|
| `Std.IO` | `IOError` (`errorCode` `message`、`Show` は `No such file or directory (os error 2)`)、`getLine : IO (Maybe String)` (行末の `\n` と `\r\n` を除く)、`getContents` (標準入力の残り)、`ePutStr` `ePutStrLn` (標準エラー出力)、`flush` |
| `Std.Fs` | `readFile` `writeFile` `appendFile` `removeFile`。開いたファイル `File` (線形。`Release` で閉じる): `openRead` `create` `openAppend : String -> IO1 (Result IOError File)`、`read` (最大 n バイト) `readAll` `write : (1 f : File) -> … -> IO1 (File, Result IOError …)`、`close` (誤りを返す) |
| `Std.Env` | `args : IO (List String)` (プログラム名を除く)、`getEnv : String -> IO (Maybe String)` |
| `Std.Process` | `exitWith` `exitSuccess` `exitFailure` (終了処理は実行されない)、`processId` |
| `Std.Time` | `now` (Unix 時刻のナノ秒)、`monotonic` (単調時計のナノ秒) |

- 標準入力は `readInt` と同じバッファを通して読む。標準エラー出力・ファイル記述子 1 への書き込みは、先に標準出力のバッファを書き出す (出力の順序を保つ)。
- 実装はランタイムの関数 (`cirka-runtime` の `stdio.rs`) を `extern "cirka"` で、libc を `extern "C"` で呼ぶ。失敗は `-errno` (または空の文字列と `errno`) で返し、ライブラリが `Result` にする。

### 8.4 診断

| コード | 内容 |
|--------|------|
| E1007 | 未知の呼び出し規約、リンクできないライブラリ |
| E1270 | 境界を越えられない型を `extern` 関数の引数・結果に書いた |
| E1271 | 同じ記号を異なる値の渡し方で宣言した |
