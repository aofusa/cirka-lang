# CirkaLang v0.0.7 言語仕様

- 版: v0.0.7 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0260 (ケイパビリティ)、FEAT-0261 (効果型の試作)、FEAT-0262 (セッション型)、FEAT-0263 (リリース)、BUG-0033 (消去される値の全域性)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.7: 効果・能力・セッション型」を確定したもの。

v0.0.7 の目的は、**関数がどの種類の外部作用を起こしうるかを型に表すこと** である。
外の資源に名前で触れる操作と非決定的な値の観測は **能力** (capability) を要し、能力は `main` が受け取って必要な関数に渡す。
あわせて効果型 (§2) とセッション型 (§3) を試作する。
この文書には [spec-v0.0.6.md](spec-v0.0.6.md) からの差分だけを書く。書いていないことは v0.0.6 のとおり。
各節は対応するチケットの完了時に書き足す。

## 0. 変更の一覧

| 節 | 内容 | チケット |
|----|------|----------|
| §1.1 | 能力の型 `Cap : Capability -> Type` と種類 `Capability` | FEAT-0260 |
| §1.2 | 能力の取得: `main` とテストの引数 | FEAT-0260 |
| §1.3 | 能力の要る標準ライブラリの操作と、要らない操作 | FEAT-0260 |
| §1.4 | OS の乱数 `Std.Random` | FEAT-0260 |
| §1.5 | 偽造できないことと信頼の境界 (`extern`) | FEAT-0260 |
| §1.6 | 実行時の表現とコスト | FEAT-0260 |
| §1.7 | 診断 (E1200 の拡張、E1117) | FEAT-0260 |
| §1.8 | v0.0.6 からの書き換え、今後 | FEAT-0260 |
| §2.1–2.2 | 効果の計算 `Eff es a` と `effect` 宣言 | FEAT-0261 |
| §2.3–2.4 | 所属 `Has e es` とハンドラ (`handle`・`handleWith`) | FEAT-0261 |
| §2.5–2.6 | 標準の効果 (`State`・`Reader`・`Writer`) と能力との関係 | FEAT-0261 |
| §2.7 | 実行時の表現とコスト (evidence passing と共通 Core の特殊化) | FEAT-0261 |
| §2.8–2.9 | 診断 (E1122・E1262・E1259・E1254)、評価 | FEAT-0261 |
| §3.1 | プロトコル `Session` と双対 `dual`、ループ (`Rec`・`Var`) | FEAT-0262 |
| §3.2 | 端点 `Channel s` と操作、始め方 (`connect`・`fork`) | FEAT-0262 |
| §3.3 | 型検査: プロトコルの違反・途中での放棄・双対でない相手が型エラー | FEAT-0262 |
| §3.4 | 実行時の表現とコスト (ライブラリだけのプリミティブ `primUnsafeCastChan`) | FEAT-0262 |
| §3.5–3.6 | 診断 (E0302・E1200 の注記)、評価 | FEAT-0262 |
| §4 | 対象外 | FEAT-0260, FEAT-0261, FEAT-0262 |
| spec-v0.0.4.md §2.4 | 消去される値も全域でなければならない (E0507。修正前は `panic` などで偽の等式を示し、型を変換できた) | BUG-0033 |

## 1. ケイパビリティ (FEAT-0260)

### 1.1 能力の型

```
data Capability = FileSystem | Network | Clock | Random | Process | Environment deriving (Eq, Show)
prim Cap : Capability -> Type          -- Prelude
```

- `Cap k` の値を **能力** と呼ぶ。`Cap` は構成子を持たず、値を得る方法は §1.2 だけである。
- 能力は **共有できる** (`Type` の値で、線形ではない)。複製・関数への受け渡し・クロージャやデータ型のフィールドへの格納は自由である。
  権限は能力を持っていることで表され、使った回数は意味を持たない。
- 標準ライブラリの操作は能力を **多重度 0** で取る (`readFile : (0 fs : Cap FileSystem) -> String -> IO (Result IOError String)`)。
  能力は型検査のためだけにあり、実行時には渡されない (§1.6)。多重度を書かない (ω の) 能力も多重度 0 の引数に渡せる。
- 利用者の関数も能力を引数に取る。`(0 fs : Cap FileSystem) ->` と書けば実行時に何も残らない。

```
countLines : (0 fs : Cap FileSystem) -> String -> IO Int
countLines fs path = do
  r <- readFile fs path
  pure (case r of
    Ok s => length (lines s)
    Err _ => 0)

main : Cap FileSystem -> IO ()
main fs = do
  n <- countLines fs "input.txt"
  print n
```

- 能力を使う動作 (`readFile fs path`) や、能力を閉じ込めた関数を渡すと、その動作だけを渡すことになる (能力そのものは渡らない)。

### 1.2 能力の取得

- `main` の型は `IO ()`、または先頭に能力の明示的な引数を並べた `Cap k1 -> ... -> Cap kn -> IO ()` である。
  ランタイムの入口が各能力を渡す。順序と重複は自由で、多重度 0 の引数も書ける (`main : (0 fs : Cap FileSystem) -> IO ()`)。
- テストは能力を引数に取れる。テストの実行器は、各テストに求める能力を渡す。

  ```
  test "reads back what it writes" (fs : Cap FileSystem) = do
    w <- writeFile fs "out.txt" "hi"
    r <- readFile fs "out.txt"
    assertEq r (Ok "hi")
  ```

  テストの引数は `(名前 : Cap K)` (`K` は `Capability` の構成子の名前。多重度も書ける) でなければならない (E1117)。
- これ以外に能力を作る方法はない (§1.5)。

### 1.3 能力の要る操作

能力は、**外の資源に名前で触れる操作** と **非決定的な値を観測する操作** に要る。

| 能力 | 操作 |
|------|------|
| `FileSystem` | `Std.Fs`: `openRead` `create` `openAppend` `readFile` `writeFile` `appendFile` `removeFile` `listDirectory` `fileStatus` `linkStatus` `makeDirectory` `removeDirectory` `rename`、`Std.Env`: `currentDirectory` `setCurrentDirectory`、`Std.Os`: `mapFile` |
| `Network` | `Std.Net`: `listen` `connect` `connectTimeout` `resolve` `bindUdp` |
| `Clock` | `Std.Time`: `now` `monotonic` |
| `Random` | `Std.Random`: `randomInt` `randomBelow` (§1.4) |
| `Process` | `Std.Process`: `spawn` `run` `output`、`Std.Os`: `sendSignal` |
| `Environment` | `Std.Env`: `getEnv`、`Std.Os`: `hostName` |

- 能力は各操作の最初の引数である (`listen net host port`、`now clk`)。
- 既に持っているハンドル (`File`・`TcpListener`・`TcpStream`・`UdpSocket`・`Child`・`Mapping`・`Signals`) の操作は能力を取らない。
  ハンドルは能力を使って得たもので、ハンドル自身がその資源への権限である。
- 次の操作は能力を要らない。
  - 標準入出力 (`putStrLn`・`getLine`・`Std.IO`) と端末の判定 (`isTerminal`)、コマンドライン引数 (`argCount`・`getArg`・`Std.Env.args`)。
    プログラムに渡されたもので、外の資源を名前で選ばない。
  - 終了 (`exit`・`panic`)、自分のプロセスの情報 (`processId`・`userId` など) と自分へのシグナル (`ignoreSignal`・`raiseSignal`・`signals` など)。
  - スレッドと同期 (`Std.Thread`)、イベントループとタイマー (`Std.Async`。時刻を値として返さない)、`cpuCount`。
  - 計算: `Std.Math`・`Linalg`・Vulkan compute (`Vulkan`)。

### 1.4 乱数 (`Std.Random`)

```
randomInt : (0 r : Cap Random) -> IO Int             -- 64 ビットの一様な乱数
randomBelow : (0 r : Cap Random) -> Int -> IO Int    -- 0 以上 n 未満の一様な乱数 (n <= 0 なら panic)
```

- OS の乱数源 (Linux の `getrandom`) から読む。暗号に使える品質である。
- `randomBelow` は棄却法で剰余の偏りを除く。

### 1.5 偽造できないことと信頼の境界

- 能力の値は `main` とテストの引数としてだけ現れる。`Cap` には構成子がなく、利用者のコードは `prim` 宣言・`trusted` を書けない (E1009)。
- 消去される位置 (多重度 0) の能力も偽造できない。消去される値は全域でなければならず (spec-v0.0.4.md §2.4、E0507)、
  `panic` や止まらない関数で `Cap k` の値を「作る」ことはできない。`primRewrite` で他の型から変換するには、その型と `Cap k` の等式の証明が要るが、
  全域な項では示せない。
- 信頼の境界は `extern` ブロックである (Rust の `unsafe` と同じ位置づけ)。`extern "C"` で libc の `open` を宣言すれば、能力なしにファイルを開ける。
  能力が保証するのは、**`extern` ブロックを書かないコードでは、§1.3 の種類の外部作用はその能力を受け取った関数だけが起こせる** ことである。
  標準ライブラリの `extern` の関数は非公開 (`private extern`) で、能力を取る公開の関数だけが使う。

### 1.6 実行時の表現とコスト

- 多重度 0 の能力は消去され、何も残らない。標準ライブラリの操作はすべて能力を多重度 0 で取るので、操作の呼び出しのコストは v0.0.6 と同じである。
- 多重度 1・ω の能力は、Core では参照カウントのない定数 (`Unit#`) である。`main` の能力の引数にはランタイムの入口が定数を渡す。
  MIR では引数が 1 つ増えるが、定数なので LLVM の最適化で伝播し、インライン化されれば消える (`cap_erased.cirka` の能力を ω にした例では、
  clang `-O2` の後に何も残らなかった)。インライン化されない関数の引数には 1 語分のコストが残りうるので、能力は多重度 0 で受け取るのがよい。
- テスト `tests/cirkalang/run/v007/cap_erased.cirka` (多重度 0 の能力を渡す) と `cap_none.cirka` (能力なし) は、`-O2` で同一の MIR になることを確かめる。

### 1.7 診断

| コード | 内容 |
|--------|------|
| E1200 | `main` の型が `IO ()` でも、能力を受け取って `IO ()` になる関数でもない (v0.0.7 で能力の引数を許した) |
| E1117 | テストの引数が能力 (`(名前 : Cap K)`) でない |

- 能力の要る操作に能力を渡さない誤りは型の不一致 (E1200) で、期待した型が `Cap k` のときは能力の受け取り方をヘルプに示す。

### 1.8 v0.0.6 からの書き換えと今後

- §1.3 の操作を使う v0.0.6 のプログラムは、`main` で能力を受け取り、操作に渡すよう書き換える。
  リポジトリのテストとベンチは書き換えた (FEAT-0263 と `CHANGELOG.md` に記録する)。
- 13-verification.md §4.1 の当初案 (線形な能力を操作ごとに返す) は採らなかった (ADR-103)。偽の資源を使うテストは効果のハンドラ (FEAT-0261) で書く。
- 今後の課題は §4 に挙げる。

## 2. 効果 (FEAT-0261、試作)

関数が使う **効果** (操作の組) を型の **行** に表し、操作の意味を、計算の外側に置く **ハンドラ** が決める。
v0.0.7 の効果は試作で、採否と最終的な形は v0.1.0 で決める (FEAT-0300)。

```
import Std.Effect

effect Counter where
  tick : Int -> ()

-- `Counter` を含むどの行でも使える (行の多相)
countdown : Has Counter es => Int -> Eff es ()
countdown n = if n == 0 then pure () else do
  tick n
  countdown (n - 1)

main : IO ()
main = do
  r <- runEff (runState 0 (handleWith (\cx => MkCounter (\n => runIn cx (modify (\s => s + n)))) (countdown 10)))
  print (snd r)                                   -- 55
```

### 2.1 効果の計算 `Eff`

```
prim Eff : List Type -> Type -> Type        -- Std.Effect
```

- `Eff es a` は、効果の行 `es` の操作を使って `a` を返す計算である。`Eff es` はモナドで、`do` で書ける。
- 行は効果の型のリストで、先頭が内側 (最も近いハンドラ) である。行の多相は行の変数と所属の制約で表す:
  `Has Counter es => Eff es ()` は「`Counter` を含むどの行でも動く」(Koka の `<counter|e>` に当たる)。
- `liftIO : IO a -> Eff es a` で `IO` の動作を使える。能力の要る操作は、`Eff` の中でも能力を要る (§1.3)。
- `runEff : Eff Nil a -> IO a` で、行が空になった (すべての効果を処理した) 計算を実行する。

### 2.2 効果の宣言

```
effect State (s : Type) where
  get : s
  put : s -> ()
```

- `effect E params where op : A1 -> ... -> An -> R` は、効果 `E` とその操作を宣言する。次の宣言に展開される (`Std.Effect` は自動で見える)。
  - 効果の型 `data E params = MkE (A1 -> ... -> IO R) ...`。値は **ハンドラ** (各操作の実装の組) である。
  - 各操作 `op : {params} -> {es : List Type} -> Has (E params) es => A1 -> ... -> An -> Eff es R`。
    最も内側の `E` のハンドラの実装を、その場で呼ぶ。
- 操作の引数と結果は `Type` の値である。`effect` の中には操作の型注釈だけを書き、同じ名前の操作を二度宣言しない。
  操作の引数は普通の矢印 `A -> B` で書く: 暗黙の引数・依存する引数・多重度は書けない (多相な操作・線形な引数は §4)。
  いずれも E1122。`private effect` は、型・構成子・操作のすべてを非公開にする。
- ハンドラは構成子 `MkE` に、各操作の実装を宣言の順に渡して作る。
- `effect` は文脈キーワードで、予約語ではない。`effect 大文字の名前 ... where` の形のときだけ宣言になる (`effect` という名前の関数は書ける)。

### 2.3 所属 `Has`

```
trait Has (e : Type) (es : List Type)      -- Std.Effect
```

- `Has e es` は、効果 `e` が行 `es` にあることを表す。辞書は、ハンドラのリストから `e` のハンドラを取り出す位置である。
  `Send` と同じく、コンパイラが行の形から解く (利用者は実装を書かない)。
  - 文脈の制約 `Has e es` (型注釈の `Has e es =>`) があれば、それを使う。
  - `es = f :: rest` で `f` が `e` と同じ型なら先頭を使う。`f` が `e` と違う型 (先頭の型構成子か、その引数が違う) なら、`Has e rest` を解く。
    同じ効果が行に二つあれば、内側 (先頭に近い方) が使われる。
    `f` が型変数 (どの効果か分からない) なら、先頭か残りかを決められないので、文脈の制約が要る。
  - `e` の引数がまだ決まっていない (`n <- get` の `State ?s`) なら、ほかの制約で決まるのを待つ。最後まで決まらなければ、
    行の中で先頭の型構成子が同じ最も内側の効果に合わせる (整数リテラルの型を `Int` に決めるより先に行う)。
- 行にない効果を使うと E1262。

### 2.4 ハンドラ

```
handle : e -> Eff (e :: es) a -> Eff es a
handleWith : (Context es -> e) -> Eff (e :: es) a -> Eff es a
runIn : Context es -> Eff es a -> IO a
```

- `handle h m` は、効果 `e` のハンドラ `h` の下で `m` を実行する。`m` の中の `e` の操作は `h` の実装を呼ぶ。
- `handleWith` のハンドラは、ハンドラを置いた所の効果 `es` を `runIn` で使える (外側の効果で内側の効果を実装する)。
- 操作は **末尾再開形** (tail-resumptive) である: 操作の実装は操作を呼んだ所でそのまま実行され、その結果が操作の結果になる。
  継続を捕らえるハンドラ (途中で抜ける、複数回再開する) は扱わない (§4)。

### 2.5 標準の効果

| 効果 | 操作 | ハンドラ |
|------|------|----------|
| `State s` | `get` `put`、`modify` | `runState : s -> Eff (State s :: es) a -> Eff es (a, s)`、`evalState` |
| `Reader r` | `ask` | `runReader : r -> Eff (Reader r :: es) a -> Eff es a` |
| `Writer w` | `tell` | `runWriter : Eff (Writer w :: es) a -> Eff es (a, List w)` (言った順) |

`State` と `Writer` のハンドラは、ランタイムの可変のセル (FEAT-0045 の `cirka_mutvar_*`) に状態を置く。セルは `Std.Effect` の外に公開しない。

### 2.6 能力との関係

- 効果は、能力の要る操作の **インターフェース** になる。本物のハンドラを能力から作り、効果を使うコードには能力を渡さない。

  ```
  effect Files where
    readText : String -> Result IOError String

  realFiles : (0 fs : Cap FileSystem) -> Eff (Files :: es) a -> Eff es a
  realFiles fs m = handle (MkFiles (\p => Fs.readFile fs p)) m
  ```

- テストでは偽のハンドラを置けば、能力なしで試せる (§1.8 の「偽の資源を使うテスト」)。
- ハンドラが操作を絞れば、能力を弱めたものとして使える (決まったディレクトリだけを読むハンドラなど。§4 の能力の弱め方の一部の代わり)。

### 2.7 実行時の表現とコスト

- `Eff es a` は、共通 Core ではハンドラのリスト (証拠のベクトル) を受け取る `IO` の newtype である。`Has e es` の辞書はリストから
  ハンドラを取り出す関数で、操作はその実装を呼ぶ (evidence passing)。行は消去される。
- 共通 Core の次の変換で、ハンドラが分かる所では、操作が通常の関数呼び出しと同じになる (06-ir.md):
  1. `IO` の引数の展開 (io-arity) が、`IO` の上の newtype (`Eff`) を一度に展開する: `f : τ̄ -> Eff es a` の本体は `f$io x̄ ev w` になる。
  2. 型の引数だけを取る辞書 (`Has` の先頭の位置) も、閉じた型で使う所では定数の辞書として特殊化する。
  3. 静的な引数の特殊化 (FEAT-0061) を、呼び出し側で値が分かる構成子の引数 (ハンドラのリスト・辞書) にも広げる。
     再帰関数が変えずに渡し `case` で調べるだけの引数は、分かっている値で特殊化し、ハンドラの実装をインライン化する。
  4. リストの中のハンドラは存在型のフィールドなので `Any` にキャストして持つ。簡約器は `Any` に詰めた構成子も既知の構成子として扱い、
     取り出したハンドラの λ を操作の位置でβ簡約する。インライン化しない大きなハンドラは、λ 持ち上げで大域関数になる。
- これらの変換はフロントエンドに依らず、aCHC の `Eff` (FEAT-0161) も同じ形にすれば使える。
- 結果として、ハンドラが分かるループの中の操作は、ハンドラを名前で呼ぶのと同じコードになる
  (`tests/cirkalang/run/v007/effect_loop.cirka` は、直接呼ぶ版と確保・参照カウント・命令の数が同じことを確かめる)。
  ハンドラが実行時に決まる所 (条件で選ぶ、データ構造に入れて渡す、再帰の中でハンドラを足す) では、リストをたどってハンドラを呼ぶ間接呼び出しが残る。

### 2.8 診断

| コード | 内容 |
|--------|------|
| E1122 | `effect` の中に操作の型注釈でないものを書いた、同じ名前の操作が二つある、操作の引数が普通の矢印でない |
| E1262 | 行にない効果の操作を使った (ハンドラを置くか、所属の制約を型注釈に加える) |
| E1259 | (既存の拡張) `Has` の実装を書いた (`Send` と同じくコンパイラが解く) |
| E1254 | (既存の拡張) 行の中で先に来る効果が型変数で、操作の効果と同じか分からない (`Has e (f :: es)` を型注釈の制約に加える) |

### 2.9 評価

- **コスト**: ハンドラが分かる所では、操作は通常の関数呼び出しと同じコードになる (§2.7)。
  `bench/effects` (2 つの効果の操作を 1 億回。Docker、hyperfine 15 回): 効果 549.6 ± 12.4 ms、ハンドラを名前で呼ぶ版 668.9 ± 15.0 ms、Rust 674.9 ± 90.6 ms
  (ホストの別の負荷で Rust の揺らぎが大きい)。最大 RSS はどれも 2 MB 以下。
  命令数 (callgrind、1000 万回) は効果 260.2 M、名前で呼ぶ版 330.2 M、Rust 250.3 M で、効果の版は Rust の 1.04 倍。
  名前で呼ぶ版の残りの差は、結合点で `()` を `drop` する命令で、効果とは別の共通の改善 (FEAT-0089) で除いた
  (名前で呼ぶ版も 260.2 M 命令。時間は効果 546.0 ± 18.7 ms、名前で呼ぶ版 557.5 ± 28.2 ms、Rust 548.8 ± 10.0 ms)。
  名前で呼ぶ版の計測で、`do` の途中の `if` が選んだ動作を反復ごとにクロージャとして作っていたことが分かり、共通 Core で直した
  (BUG-0034。名前で呼ぶ版はホストで 5.9 s → 0.66 s)。
- **書き方**:
  - 行の多相 (`Has E es =>`) と `effect` 宣言で、末尾再開形の効果 (状態・環境・出力・外の資源へのインターフェース) を、
    型システムに行の単一化を入れずに書けた。
  - `get` の状態の型は行から推論される (§2.3 の改善)。改善がないと、状態の型を型注釈で書くか、
    `n <- get; put (n + 1)` の状態がリテラルの既定の型 (`Int`) に決まってしまう (行の状態が `Float` なら型が合わない)。
  - ハンドラを構成子に宣言の順で渡す書き方は、操作が多いと読みにくい (名前で書く構文は §4)。
  - 外側の効果を使うハンドラは `handleWith` と `runIn cx` を書く必要があり、ハンドラの中で外側の効果をそのまま使える Koka より重い。
  - 行の先頭が型変数の効果だと、どのハンドラを使うか決まらず、制約を書く必要がある (E1254)。
- **能力との関係**: 効果は能力の要る操作のインターフェースになり、本物のハンドラを能力から作り、テストでは偽のハンドラに替えられた
  (`run/v007/effect_files.cirka`、`test/effects.cirka`)。能力を渡すのはハンドラを置く所だけになる。
- **v0.1.0 への持ち越し** (§4): 継続を捕らえるハンドラ、線形な値と効果、純粋な実行、名前で書くハンドラ、外側のハンドラを使う操作 (`mask`)。

## 3. セッション型 (FEAT-0262、試作)

二つのスレッドの間のチャネルを、**型に書いたプロトコルのとおりにしか使えない** ようにする。
プロトコルは型の中で使う値 (依存型) で、相手のプロトコルは関数 `dual` で計算する。チャネルの端点は線形で、プロトコルの終わり (`End`) でしか捨てられない。
このため、プロトコルにない操作、端点の二重の使用、途中での放棄は型エラーになる。
言語に構文や型の規則は加えず、ライブラリ `Std.Session` と、ライブラリだけが宣言できるプリミティブ 1 つで作る。

```
import Std.Session as S

adder : S.Session                                       -- 和を求める側のプロトコル
adder = S.Rec (S.Choose (S.Send Int (S.Send Int (S.Recv Int (S.Var 0)))) S.End)

sums : List (Int, Int) -> (1 c : S.Channel adder) -> IO Int
sums [] c = do
  c <- S.chooseRight (S.enter c)                        -- 終える
  S.close c
  pure 0
sums ((x, y) :: ps) c = do
  c <- S.chooseLeft (S.enter c)                         -- もう一回
  c <- S.send c x
  c <- S.send c y
  (n, c) <- S.recv c
  r <- sums ps c
  pure (n + r)

serve : Int -> (1 c : S.Channel (S.dual adder)) -> IO Int
serve k c = do
  b <- S.offer (S.enter c)
  case b of
    S.Left c => do
      (x, c) <- S.recv c
      (y, c) <- S.recv c
      c <- S.send c (x + y)
      serve (k + 1) c
    S.Right c => pure k                                 -- 終わった端点は捨てられる (閉じる)

main : IO ()
main = do
  (t, k) <- S.connect (sums [(1, 2), (3, 4)]) (serve 0)
  print (t, k)                                          -- (10, 2)
```

### 3.1 プロトコル

```
data Session : Type where
  Send : (0 a : Type) -> (s : Session) -> Session       -- a を送り、s をする
  Recv : (0 a : Type) -> (s : Session) -> Session       -- a を受け取り、s をする
  Choose : (s : Session) -> (t : Session) -> Session    -- s か t を選ぶ (相手は両方を用意する)
  Offer : (s : Session) -> (t : Session) -> Session     -- 相手が選んだ方をする
  Rec : (s : Session) -> Session                        -- ループ: s の中の Var 0 がループ自身
  Var : (k : Int) -> Session                            -- k 段外側のループ
  End : Session                                         -- 終わり

dual : Session -> Session                               -- 相手のプロトコル
unroll : Session -> Session                             -- ループの 1 周 (unroll s = subst 0 (Rec s) s)
```

- プロトコルは `Session` の値で、型の中で使う。型の中の式は型検査で評価される (spec-v0.0.4.md §1.4)。
  送る値の型は多重度 0 のフィールドなので消去され、`Session` の値は実行時には使われない。
- `dual` は、送ると受け取るを、選ぶと用意するを入れ替える。`dual` とループの置き換え `subst` は構造の再帰で全域であり、型の中で使える (E0505 にならない)。
- ループは de Bruijn 添字で書く。`Rec s` の中の `Var 0` は `Rec s` 自身、`Var 1` は一つ外側の `Rec` である。
  `unroll s` は `s` の中の `Var 0` を `Rec s` で置き換えた 1 周分で、ループに入るとき (`enter`) に端点の型がこれになる。
  外側に対応する `Rec` のない `Var k` の端点には操作がなく、捨てることもできない。
- 構成子の名前 `Send` は Prelude のトレイト `Send` (spec-v0.0.5.md §1.1) と、`Left` / `Right` (§3.2) は `Either` の構成子と同じである。
  `Std.Session` は `import Std.Session as S` のように修飾して使う (修飾せずに両方を見せて名前を使うと E1102)。

### 3.2 チャネルと操作

```
data Channel (s : Session) : LType                      -- プロトコル s をする端点 (構成子は非公開)
impl Drop (Channel End)                                 -- 終わった端点だけが捨てられる (閉じる)

send : Send a => (1 c : Channel (Send a s)) -> a -> IO1 (Channel s)     -- 待たない
recv : (1 c : Channel (Recv a s)) -> IO1 (a, Channel s)                -- 届くまで待つ
chooseLeft : (1 c : Channel (Choose s t)) -> IO1 (Channel s)
chooseRight : (1 c : Channel (Choose s t)) -> IO1 (Channel t)
offer : (1 c : Channel (Offer s t)) -> IO1 (Branch s t)                -- 相手が選ぶまで待つ
enter : (1 c : Channel (Rec s)) -> Channel (unroll s)                  -- ループに入る (実行時には何もしない)
close : (1 c : Channel End) -> IO ()

data Branch (s : Session) (t : Session) : LType where                  -- 相手が選んだ方
  Left : (1 c : Channel s) -> Branch s t
  Right : (1 c : Channel t) -> Branch s t

connect : Send b => ((1 c : Channel s) -> IO a) -> ((1 c : Channel (dual s)) -> IO b) -> IO (a, b)
fork : Send b => ((1 c : Channel s) -> IO b) -> IO1 (Channel (dual s), Ur (Thread b))
```

- 操作は端点を受け取り、プロトコルの残りをする端点を返す。端点は線形なので、次の操作には返された端点を使う
  (`c <- S.send c x` のように同じ名前で束縛し直す)。
- 送る値は `Send` (spec-v0.0.5.md §1.1) でなければならず、相手のスレッドに移る。送れるのは `Type` の値だけで、
  線形な値 (端点を含む) は送れない (委譲は §4)。
- `connect self peer` は新しいチャネルを作り、その二つの端点で `peer` を新しいスレッドで、`self` をこのスレッドで実行する。
  両方の終わりを待ち、結果の組を返す。`peer` の端点のプロトコルは `self` の端点のプロトコルの双対である。
- `fork peer` は `peer` を新しいスレッドで始め、もう一方の端点とスレッドを返す (スレッドは `join` で待つ)。
  一つのスレッドが複数の相手と、順番を混ぜて話せる。スレッドは `Ur` に入れて返す (線形な組の要素は線形になるため。spec-v0.0.2.md §5.6)。
- 相手の端点は相手のスレッドの中で作る。端点を捕らえた動作を `spawn` に渡すと、何度も実行されうる位置に線形な値を置くことになる (E0305)。

### 3.3 型検査

次の誤りは型エラーになる。セッション型のための検査はなく、依存型と線形型の既存の規則で決まる。

| 誤り | 診断 |
|------|------|
| プロトコルと違う型の値を送る、違う順に操作する、途中で `close` する、相手の側の操作をする (`Choose` の端点に `offer`) | E1200 |
| 端点を使わずに捨てる (途中での放棄)。分岐の片方だけで捨てる場合と、任意のプロトコルの端点 (`Channel s`) を捨てる場合も | E0302 |
| 端点を二度使う | E0303 |
| `connect` / `fork` の二つの側のプロトコルが双対でない | E1200 |

- **双対性**: `connect` と `fork` は相手の端点に `dual s` を与える。相手のプロトコルを書き下した場合
  (`Channel (Rec (Offer (Recv Int ..) End))`) は、型検査が `dual s` を評価して比べる (定義による等しさ)。
- **放棄**: 終了処理を持つ型の線形な値は、`Drop` か `Release` が解決できるときだけ使わずに捨てられる (spec-v0.0.2.md §5.3)。
  `Channel` の `Drop` は `Channel End` にだけあるので、`End` でない端点を捨てると E0302 になる。プロトコルが型変数の端点 (`Channel s`) も、
  `End` だと分からないので捨てられない。`End` の端点は `close` しなくても、使わなければその時点で閉じる。
- **相手の失敗**: スレッドの `panic` はプロセスを終える (spec-v0.0.5.md §1.3) ので、相手が途中で止まって受け取りを待ち続けることはない。
- 一つのチャネルの二つの端点は双対のプロトコルをするので、互いに受け取りを待ち合うことはない。複数のチャネルをまたいで待ち合う
  (二つのスレッドがそれぞれ別のチャネルで相手を待つ) デッドロックは型では防げない (§4)。

### 3.4 実行時の表現とコスト

- 端点は `Std.Thread` の FIFO チャネル 2 つ (送る側と受け取る側。spec-v0.0.5.md §1.4) を持つ線形なデータで、プロトコルは型の中にしかない。
  `send` / `recv` はチャネルへの送受信 1 回、`chooseLeft` / `chooseRight` / `offer` は真偽値 (静的なオブジェクト) の送受信 1 回で、
  `enter` / `close` は何もしない。
- 二つのチャネルはプロトコルのすべての段の値を運ぶ。ライブラリだけが宣言できるプリミティブ
  `primUnsafeCastChan : {a : Type} -> {b : Type} -> Chan a -> Chan b` (利用者のプログラムには書けない。BUG-0023) で、
  チャネルの要素の型を段ごとに変える。共通 Core ではチャネルは要素の型によらずランタイムのオブジェクト (`Any`) なので、変換は何もしない。
  受け取る側が送られた型を期待することは、プロトコルの双対性が保証する。
- 端点はループの引数としては 2 つのチャネルに展開され (worker/wrapper)、操作のたびに作り直されない。
  `offer` は選ばれた側の端点を `Branch` に包んで返すが、呼び出し側がすぐに `case` で分解するので、共通 Core の case-of-case が
  どちらも作らない形にする (FEAT-0090、§3.6)。

### 3.5 診断

| コード | 内容 |
|--------|------|
| E0302 | (既存の拡張) `Drop` / `Release` の実装が別の引数にだけある型の値 (終わっていない端点) を捨てた。注記はその旨を示し、導出を案内しない |
| E1200 | (既存の拡張) 型の中の関数を評価すると形が変わる場合、評価した形 (`dual p` が計算したプロトコル) も示す |

### 3.6 評価

- **コスト**: `bench/session` (2 つのスレッドの間の 10 万往復。1 往復は、続けるかの選択・数・その答えの 3 つの値。Docker、hyperfine 15 回):
  セッション型の版 1.192 ± 0.090 s、同じ値を `Std.Thread` のチャネルで直接送る版 1.363 ± 0.265 s、Rust (`std::sync::mpsc`) 1.093 ± 0.087 s。
  時間の大半はスレッドを起こすカーネルの時間 (system 0.75〜0.88 s) で、版の違いは揺らぎの範囲に収まる。最大 RSS はどれも 2.2 MB。
  - 確保はどちらの版も 1 往復 2 つ (送る `Int` の箱)。最初はセッション型の版が 4 つで、多い 2 つは `offer` が返す `Branch` と端点だった。
    呼び出し側がすぐに分解するのに、分岐の結果が `let` の下の構成子で、`IO1` の束縛が自分の型への `cast` を残したため、
    共通 Core の case-of-case が働かなかった。FEAT-0090 でこの 2 つを除いた (時間は変わらない)。
    送る側のループでは、端点は 2 つのチャネルの引数に展開され、直接の版と同じ処理になる。
  - 命令数 (callgrind、10 万往復。同じ版の 2 回の計測の差は 0.01 % 未満) は、セッション型の版 256.7 M (FEAT-0090 の前は 312.9 M)、
    直接の版 252.4 M、Rust 147.7 M。残る 4 M の差は受け取る側の分岐の形の違いである
    (valgrind の下ではランタイムのアロケータが `malloc` に切り替わるので、確保と解放の命令は実際の実行より多い。10-testing.md)。
    直接の版と Rust の差は、ランタイムのチャネル (ロックと条件変数) と送る `Int` の箱によるもので、セッション型によらない。
- **書き方**:
  - プロトコルを型に書けば、操作の順・値の型・選択・終わり方が型検査で確かめられ、相手の型は `dual` で得られる。
    依存型 (型の中で評価される関数) と線形型と、型の引数で選ばれる `Drop` の実装で、言語に規則を足さずにライブラリとして書けた。
  - 誤りは誤った操作の位置で報告される。双対でない相手は、評価した形を示す注記で違いが分かる (§3.5)。
  - 1 段ごとに端点を受け取って返すので、`c <- S.send c x` のように同じ名前で束縛し直す。`do` の中では `IO1` の束縛でそのまま書ける。
  - ループは `enter` を書く必要があり、de Bruijn 添字の `Var k` は入れ子のループで読みにくい。選択は二択で、多くの選択肢は入れ子にする。
  - 終わった端点は捨てれば閉じる (`Drop (Channel End)`) ので `close` は省ける。終わっていない端点を捨てると E0302 になる。
- **v0.1.0 への持ち越し** (§4): 委譲 (線形な値を送る)、名前の付いた選択、送った値に依存するプロトコル、多者間のセッション型、
  チャネルをまたぐデッドロックの検出。言語に組み込むか (専用の構文・再帰の型の等価性) は、これらの評価とあわせて決める。

## 4. 対象外

v0.0.7 で扱わないもの。

- 能力の弱め方 (特定のディレクトリだけの `FileSystem`、読み出しだけの `FileSystem` など) と取り消し。能力に実行時の表現が要る。
- パッケージ単位で `extern` ブロックを禁止する設定 (§1.5 の信頼の境界を、依存するパッケージについて強制すること)。
- 標準入出力・コマンドライン引数・スレッド・タイマーの能力 (§1.3。プログラムに渡されたもので、外の資源を名前で選ばない)。
- aCHC への取り込み (H7、FEAT-0160, FEAT-0161)。
- 継続を捕らえる効果のハンドラ: 途中で抜ける操作 (例外。アンワインド FEAT-0040 の後)、複数回再開する操作 (非決定性)、
  ハンドラの中で継続を保存する操作。継続に線形な値が入る場合の規則とあわせて v0.1.0 で評価する。
- 線形な値を引数・結果に取る操作、`IO1` の上の効果。
- 純粋な実行 (`Eff Nil a -> a`)。`IO` の動作を効果として行に表し (effectful の `IOE`)、ハンドラの実装も行で型付けする必要がある。
- 名前で操作の実装を書くハンドラの構文 (`handler State where get = ...`)。v0.0.7 は構成子に宣言の順で渡す。
- 同じ効果が行に二つあるとき、外側のハンドラを使う操作 (Koka の `mask`)。v0.0.7 の操作は常に最も内側のハンドラを使う。
- 関数の型ごとに効果の行を推論すること (Koka の `a -> <e> b`)。
- セッション型 (§3) の拡張: 線形な値 (端点を含む) を送ること (委譲。共通 Core のチャネルは共有できる値 `Any` を運ぶので、線形な値の表現が要る)、
  名前の付いた多肢の選択、送った値に依存するプロトコル (続きを `(x : a) -> Session` で表す)、多者間のセッション型、
  複数のチャネルをまたぐデッドロックの検出、容量のあるチャネル (送る側も待つ)、ループの `enter` を書かずに済ませること。
