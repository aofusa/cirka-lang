# CirkaLang v0.0.5 言語仕様

- 版: v0.0.5 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0240 (スレッドと共有)、FEAT-0241 (ネットワークと非同期 I/O)、FEAT-0242 (OS 層)、FEAT-0244 (core の大きさの契約)、FEAT-0243 (リリース)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.5: 並行性・サーバ・syscall」を確定したもの。

v0.0.5 の目的は、GC なしのメモリ安全と単一スレッドの性能を保ったまま、**スレッド・ネットワークのサーバ・OS の機能** を使うプログラムを
書けるようにすることである。並行性のランタイムは共通基盤 ([05-runtime.md](../05-runtime.md) §9・§10) を使う。
この文書には [spec-v0.0.4.md](spec-v0.0.4.md) からの差分だけを書く。書いていないことは v0.0.4 のとおり。
各節は対応するチケットの完了時に書き足す。

## 0. 変更の一覧

| 節 | 内容 | チケット |
|----|------|----------|
| §1.1 | `Send`: 別のスレッドへ渡せる型。型の構造から決まる | FEAT-0240 |
| §1.2 | `ThreadBound`: 作ったスレッドでしか使えない型の印 | FEAT-0240 |
| §1.3 | `Std.Thread`: `spawn` / `join` / `threadId` / `yield` / `cpuCount` | FEAT-0240 |
| §1.4 | 同期: `MVar`・`Chan`・`Mutex` | FEAT-0240 |
| §1.5 | 実行時の表現 (共有マーク) と MIR インタプリタ | FEAT-0240 |
| §1.6 | 診断 E1259 / E1261 | FEAT-0240 |

## 1. スレッドと共有 (FEAT-0240)

### 1.1 `Send`

```
trait Send (a : LType)
```

- `Send a` は、型 `a` の値を別のスレッドへ渡せることを表す、メソッドのないトレイトである (`Prelude`)。
  渡すとは、線形な値なら所有を移すこと、ω の値なら複数のスレッドで共有することである。
- `Send` の `impl` は書けない (E1259)。型の構造から次の規則で決まる (Rust の auto trait にあたる)。
  - プリミティブの型 `Int` `UInt` `Float` `Char` `String`、型そのもの (宇宙の要素) と、関数の型 (`IO a` を含む) は `Send`。
  - 型を引数に取るプリミティブの型 (`Array a`・`UArray a`・`Unique a`・`Lazy a`・`Thread a`・`MVar a`・`Chan a` など) は、
    型である引数がすべて `Send` なら `Send`。ただし動作の型 (`IO a`・`IO1 a`・`RIO r a`・`Borrow`・`BorrowIO`) は関数なので常に `Send`。
  - データ型は、すべての構築子のすべてのフィールド (多重度 0 のものも含む) の型が `Send` なら `Send`。
    型パラメータは実際の引数で置き換えて調べる。添字のうち、構築子の結果の型がフィールドをそのまま置いている添字
    (`VCons : a -> Vect n a -> Vect (S n) a` の `a`) も実際の引数で置き換える。ほかのフィールドの値は未知として扱う。
  - 調べている型の再帰的な出現は、引数が (未知の値を除いて) 同じなら `Send` とみなす (最大不動点)。
    引数を大きくしながら自分を含む型 (`data Nest a = Done | More a (Nest (List a))`) は 16 段で調べるのをやめ、`Send` でないとする
    (Rust も同じ型で要求の評価があふれる)。
  - 型が値で決まるフィールド (存在型、`MkDyn : (0 t : Type) -> (x : t) -> Dyn` の `x`) は `Send` でない。
    ただし多重度 0 のものは実行時に何も残らないので調べない。
  - 型変数 `a` は、文脈に `Send a` の制約 (またはそれをスーパートレイトに持つ制約) があるときだけ `Send`。
  - `ThreadBound` (§1.2) は `Send` でない。
- 関数の型を `Send` とするのは、関数の型が捕捉した値を表さないためである。捕捉した値も、スレッドへ渡るときにランタイムが
  すべて共有マークするのでメモリ安全は保たれる (§1.5)。スレッドに縛られたランタイムの資源 (イベントループ) は、使うときに持ち主の
  スレッドを確かめ、違えばプログラムを終了する (05-runtime.md §10.1)。捕捉する値の静的な検査は第二段階で検討する (ADR-088)。
- Rust の `Sync` にあたるトレイトは持たない。ω の値は不変か、ランタイムが同期する (`MVar`・サンクの評価・共有された参照数) ので、
  別のスレッドへ渡せる ω の値は複数のスレッドで同時に使ってよい。
- `Send` の辞書はフィールドを持たず、特殊化で消える。制約は実行時のコストを持たない。

### 1.2 `ThreadBound`

```
prim ThreadBound : Type
prim threadBound : ThreadBound
```

- 作ったスレッドでしか使えない資源を包む型は、`ThreadBound` のフィールドを持たせて `Send` でなくする
  (Rust の `PhantomData<*const ()>` にあたる)。多重度 0 のフィールドにすれば実行時には何も残らない。

```
data Window : LType where
  MkWindow : (0 _ : ThreadBound) -> Ptr -> Window

open : IO1 Window
open = do
  p <- createWindow
  pure1 (MkWindow threadBound p)
```

### 1.3 `Std.Thread`

```
Thread : Type -> Type
spawn : Send a => IO a -> IO (Thread a)
join : Thread a -> IO a
threadId : IO Int
yield : IO ()
cpuCount : IO Int
```

- `spawn act` は `act` を新しい OS のスレッドで実行し、そのハンドルを返す。`join t` はスレッドが終わるのを待ってその結果を返す。
  何度 `join` してもよく、同じ値を返す。結果の型は `Type` に限る (線形な値は §1.4 の `MVar` でも渡せない。v0.0.5 の制限)。
- main のスレッドが終わるとプロセスは終わる。ほかのスレッドは待たない (Haskell と同じ)。待つなら `join` する。
- スレッドの `panic` はプロセスを終える (アンワインドは FEAT-0040 の後)。
- `threadId` は呼んだスレッドの番号 (main は 1)。`yield` はほかのスレッドに CPU を譲る。`cpuCount` はオンラインのプロセッサの数。
- スレッドのスタックの大きさは `CIRKA_THREAD_STACK_SIZE` (既定 64 MiB)。

### 1.4 同期

```
MVar : Type -> Type
newMVar : Send a => a -> IO (MVar a)
newEmptyMVar : Send a => IO (MVar a)
takeMVar : MVar a -> IO a
putMVar : MVar a -> a -> IO ()
readMVar : MVar a -> IO a
tryTakeMVar : MVar a -> IO (Maybe a)
tryPutMVar : MVar a -> a -> IO Bool
modifyMVar : MVar a -> (a -> IO (a, b)) -> IO b

Chan : Type -> Type
newChan : Send a => IO (Chan a)
send : Chan a -> a -> IO ()
recv : Chan a -> IO a
tryRecv : Chan a -> IO (Maybe a)
chanLength : Chan a -> IO Int

Mutex : Type -> Type
newMutex : Send a => a -> IO (Mutex a)
withMutex : Mutex a -> (a -> IO (a, b)) -> IO b
readMutex : Mutex a -> IO a
```

- `MVar a` は値を 0 か 1 つ持つ箱で、Haskell の `MVar` と同じ意味である。空の `MVar` の `take` と、満ちた `MVar` の `put` は待つ。
  `readMVar` は取り出さずに読む。`modifyMVar m f` は値を取り出して `f` を実行し、結果の組の 1 つめを戻して 2 つめを返す。
- `Chan a` は上限のない FIFO のチャネルである。`send` は待たない。`recv` は値が来るまで待つ。
- `Mutex a` は値を守る排他ロックで、`MVar` の上に作る。`withMutex m f` はロックを取って `f` を実行し、`f` の返した新しい値を戻して
  ロックを放す。`readMutex` は今の値を読む。
- ほかのどのスレッドも起こせない待ちは `thread blocked indefinitely in an MVar operation` でプロセスを終える (05-runtime.md §9.4)。
- `Send a` は作るときに要求する。作った `MVar a`・`Chan a`・`Mutex a` は `a` が `Send` なので §1.1 の規則で `Send` である。

### 1.5 実行時の表現と実行

- `Thread a`・`MVar a`・`Chan a` はランタイムの同期オブジェクト (kind `MVAR`。05-runtime.md §9) で、`spawn` は `cirka_thread_spawn`、
  `MVar` は `cirka_mvar_*`、`Chan` は `cirka_chan_*` を呼ぶ。`tryTakeMVar`・`tryRecv` は、空のときの既定値に `MVar`・`Chan` の
  オブジェクト自身を渡して返り値と比べる (型 `a` の値が `MVar a` 自身であることはない)。
- 別のスレッドから見えるようになる値 (`spawn` の動作とそれが捕捉した値、`putMVar`・`send` の値、`join` の結果) は、
  ランタイムが渡す直前に共有マークする。最初の `spawn` まではプロセスは単一スレッドで、アトミック操作もロックも払わない。
- MIR インタプリタはスレッドを実行しない (`spawn` は実行時エラー)。`MVar`・`Chan` は 1 つのスレッドの意味で動き、待つことになれば
  上の文言で終わる。スレッドを使う実行テストはネイティブだけで行う (`skip-backend: interp`。解放されずに残るオブジェクトの数は
  デバッグ用ランタイムの実行を基準にする、10-testing.md)。
- ThreadSanitizer の構成でも実行するテスト (`tsan: 1`) でデータ競合がないことを確かめる (10-testing.md)。
- `threadId`・`yield`・`cpuCount` は `extern "cirka"` でランタイムを呼ぶ。ほかは型 `a` の値を渡すのでプリミティブ (`primSpawn` など、
  `Std.Thread` の外からは使えない) で、`Send` の辞書は特殊化で消える。

### 1.6 診断

| コード | 内容 |
|--------|------|
| E1259 | `Send` の `impl` を書いた (型の構造から決まる) |
| E1261 | 値を別のスレッドへ渡せない (`Send` でない)。注に `Send` でない部分 (`ThreadBound` のフィールド、制約のない型変数) を示す |
