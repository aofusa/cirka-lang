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
| §2.1 | `Handle` と、ランタイムの関数へ関数・動作を渡すこと | FEAT-0241 |
| §2.2 | `Std.Async`: イベントループと一度だけ実行される継続 | FEAT-0241 |
| §2.3 | `Std.Net`: TCP・UDP のソケット (線形な値)、タイムアウト | FEAT-0241 |
| §2.4 | サンプル: HTTP/1.1 のエコーサーバ | FEAT-0241 |
| §2.5 | 実行時の表現と実行、制限、診断 | FEAT-0241 |
| §3.1 | errno と `Result`: C の関数の結果 (`cResult`)、エラー番号の定数 | FEAT-0242 |
| §3.2 | 子プロセス (`Std.Process`): `Command`、線形な `Child` (捨てると終わりを待つ)、`output`、`kill` | FEAT-0242 |
| §3.3 | シグナル (`Std.Os`): 無視・ブロック・待ち、イベントループで受け取る `Signals` | FEAT-0242 |
| §3.4 | ファイルとディレクトリ (`Std.Fs`・`Std.Env`): 一覧・属性・作成・削除・改名、作業ディレクトリ | FEAT-0242 |
| §3.5 | メモリマップ (`Mapping`)、プロセスとホストの情報 | FEAT-0242 |
| §3.6 | サンプル: OS 層を使う CLI ツール | FEAT-0242 |
| §3.7 | 実行時の表現と実行、制限 | FEAT-0242 |

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

## 2. ネットワークと非同期 I/O (FEAT-0241)

多数の接続は、スレッドごとのイベントループ (05-runtime.md §10) の上で、待つ操作に **残りの処理を一度だけ実行される継続として渡して** 書く
(ADR-089)。言語に `async` も効果 (v0.0.7) もないので、継続はふつうの関数で、多重度 1 の引数として受け取る。
ソケットは線形な値で、操作はソケットを受け取って継続に返す。同じソケットを同時に 2 つの操作に使うことは型で防がれ、
使い終わったソケットは捨てれば閉じる。

### 2.1 `Handle` とランタイムの関数

```
prim Handle : Type          -- Foreign
```

- `Handle` はランタイムのオブジェクト (ソケットの記述子、イベントループの受け口) を指す不透明な型である。CirkaLang の値を持たないので
  資源を持たない ω の型として扱い、`Send` である。所有を表すのはライブラリの線形な型 (`TcpStream` など) で、`Handle` はその中身にする。
  ほかの値と同じく参照数で数え、最後の参照が消えるとランタイムが後始末する (記述子なら閉じる)。
- `extern "cirka"` (ランタイムの規約。spec-v0.0.2.md §8.1) の関数は、`String` に加えて `Handle` を取り・返せる。
  また **関数と動作** (`a -> IO b`・`IO a`・`IO1 a` の引数) を取れる。クロージャのオブジェクトを借用で渡し、ランタイムが持ち続けるときは
  自分で参照を増やす。多重度 1 の引数 (`(1 k : IO ())`) は、ランタイムがちょうど 1 回だけ呼ぶと約束する継続である
  (呼び出す側から見ればふつうの線形な引数で、渡せば消費したことになる)。
- `extern "C"` には `Handle` も関数・動作も渡せない (E1270)。

### 2.2 `Std.Async`

```
runLoop : (1 act : IO ()) -> IO ()
after : Int -> (1 k : IO ()) -> IO ()
defer : (1 k : IO ()) -> IO ()
data Timer
startTimer : Int -> IO () -> IO Timer
cancelTimer : Timer -> IO Bool
stopLoop : IO ()
blocking : Send a => (1 work : IO a) -> (1 k : a -> IO ()) -> IO ()
data Remote
remote : IO Remote
post : Remote -> IO () -> IO Bool
```

- `runLoop act` はこのスレッドに新しいイベントループを作って `act` を最初のコールバックとして実行し、待つもの (ソケットの待ち・タイマー・
  `blocking` の仕事・生きている `Remote`) がなくなるか `stopLoop` が呼ばれるまでループを回す。終わるとループは解放される。
  入れ子の `runLoop` は内側のループを回す。ほかの関数は、このスレッドで動いているいちばん内側のループに働き、ループの外で呼ぶと
  プログラムを終える (`no event loop is running on this thread`)。
- 待つ操作は、残りの処理を **継続** `k` (多重度 1) として受け取ってすぐに戻る。`k` はループのスレッドで **ちょうど 1 回** 実行される:
  操作が終わったとき、失敗したとき、タイムアウトしたときのいずれか。ただし `stopLoop` は、待っている継続を実行せずに捨てる。
- `after ms k` は `ms` ミリ秒後に、`defer k` は今実行できるコールバックの後 (次のターン) に `k` を実行する。期限が同じなら登録した順。
- `startTimer ms act` は取り消せるタイマーで、`cancelTimer t` はまだ実行していなければ取り消して `True` を返す。
  取り消すと `act` は実行されないので、`act` は継続ではなく ω の動作である (線形な値を捕捉できない)。
- `blocking work k` は、ブロックする処理 `work` (名前解決など) をランタイムのスレッドプールで実行し、その結果について `k` を
  ループのスレッドで実行する。結果はスレッドを越えるので `Send a` を要求する。
- `Remote` は、ほかのスレッドからループへ動作を送る受け口である。`post r act` はどのスレッドからでも呼べ、`act` をループのスレッドで
  実行させる (ループがもうなければ `False`)。`Remote` が生きている間は、まだ送られてくるかもしれないのでループは終わらない。
  ループのスレッドは受け口をほかのスレッドへ渡し、自分の手元からは捨てる (05-runtime.md §10.1)。

### 2.3 `Std.Net`

```
data TcpListener : LType
data TcpStream : LType
data UdpSocket : LType
data Datagram = MkDatagram String String Int     -- データ、送り手のアドレス、送り手のポート
timedOut : IOError

listen : String -> Int -> IO1 (Result IOError TcpListener)
listenerPort : (1 l : TcpListener) -> IO1 (TcpListener, Int)
accept : (1 l : TcpListener) -> (1 k : (1 l : TcpListener) -> (1 r : Result IOError TcpStream) -> IO ()) -> IO ()
acceptTimeout : (1 l : TcpListener) -> Int -> (1 k : (1 l : TcpListener) -> (1 r : Result IOError TcpStream) -> IO ()) -> IO ()
serve : (1 l : TcpListener) -> ((1 s : TcpStream) -> IO ()) -> IO ()
closeListener : (1 l : TcpListener) -> IO ()

connect : String -> Int -> (1 k : (1 r : Result IOError TcpStream) -> IO ()) -> IO ()
connectTimeout : String -> Int -> Int -> (1 k : (1 r : Result IOError TcpStream) -> IO ()) -> IO ()
recv : (1 s : TcpStream) -> Int -> (1 k : (1 s : TcpStream) -> Result IOError String -> IO ()) -> IO ()
recvTimeout : (1 s : TcpStream) -> Int -> Int -> (1 k : (1 s : TcpStream) -> Result IOError String -> IO ()) -> IO ()
sendAll : (1 s : TcpStream) -> String -> (1 k : (1 s : TcpStream) -> Result IOError () -> IO ()) -> IO ()
sendAllTimeout : (1 s : TcpStream) -> String -> Int -> (1 k : (1 s : TcpStream) -> Result IOError () -> IO ()) -> IO ()
shutdownWrite : (1 s : TcpStream) -> IO1 TcpStream
setNodelay : (1 s : TcpStream) -> Bool -> IO1 TcpStream
close : (1 s : TcpStream) -> IO ()
resolve : String -> (1 k : Result IOError String -> IO ()) -> IO ()

bindUdp : String -> Int -> IO1 (Result IOError UdpSocket)
udpPort : (1 u : UdpSocket) -> IO1 (UdpSocket, Int)
sendTo : (1 u : UdpSocket) -> String -> Int -> String -> (1 k : (1 u : UdpSocket) -> Result IOError () -> IO ()) -> IO ()
recvFrom : (1 u : UdpSocket) -> Int -> (1 k : (1 u : UdpSocket) -> Result IOError Datagram -> IO ()) -> IO ()
recvFromTimeout : (1 u : UdpSocket) -> Int -> Int -> (1 k : (1 u : UdpSocket) -> Result IOError Datagram -> IO ()) -> IO ()
closeUdp : (1 u : UdpSocket) -> IO ()
```

- ソケットは線形な値 (`LType`) で、`Send` である (別のスレッドのループへ渡せる)。操作はソケットを受け取り、継続へ返す。
  同じソケットを 2 つの操作に渡すこと (E0303) と、何度も実行される関数に捕捉させること (E0305) は型の誤りになる。
  `close`・`closeListener`・`closeUdp` はその場で閉じ、捨てたソケット (`stopLoop` で捨てられた継続が持っていたものを含む) は解放のときに閉じる。
- アドレスは IPv4・IPv6 の数値アドレスで、空のホストは IPv4 の任意のアドレス、ポート 0 は空いているポートを選ぶ。
  `connect` は数値でないホストを `resolve` で解決してからつなぐ。`resolve` は `getaddrinfo` の最初のアドレスを文字列で返し、
  スレッドプールで実行する (`blocking`)。
- 失敗は `Err` の値で返す (`IOError` は errno で、`show` は OS の文言)。`listen`・`bindUdp` の失敗はその場で、ほかは継続に渡す。
- `…Timeout` の版は、待ちが `ms` ミリ秒を超えると `Err timedOut` (`ETIMEDOUT`) を継続に渡す。ソケットは使えるまま返る
  (`recvTimeout` の後にまた `recv` できる)。`sendAllTimeout` の `ms` は、書く余地を待つ 1 回ごとの上限である。
- `recv s n k` は最大 `n` バイト (65536 まで) を受け取り、流れの終わりでは空の文字列を渡す。`sendAll` はすべてを送り終えてから継続する。
  `shutdownWrite` は送る側を閉じる (相手には流れの終わりが見え、受け取りは続けられる)。`setNodelay` は Nagle のアルゴリズムを切る。
- `serve l handler` は接続を受け付け続け、接続ごとに `handler` を始める。`handler` は何度も実行されるので ω の関数である。
  `accept` の失敗 (記述子が足りないなど) は 10 ミリ秒後にやり直す。
- UDP: `sendTo u host port d k` は `d` を 1 つのデータグラムとして `host` (数値) の `port` へ送る。`recvFrom u n k` はデータグラムを 1 つ受け取り、
  その最初の `n` バイトと、送り手のアドレスとポートを渡す。
- 公平性: 1 つのコールバックの中ですぐに完了する操作 (データが届いている `recv`、書ける `sendAll`、待っている接続がある `accept` など) は
  64 回まで (`BUDGET`)。それを超えた操作はループを待つ。データが絶え間なく届く接続が、ほかの接続を待たせ続けることはない。

### 2.4 サンプル: HTTP エコーサーバ

`tests/cirkalang/run/v005/http_echo.cirka` は HTTP/1.1 のエコーサーバである。`http_echo PORT` は 127.0.0.1 の `PORT` で待ち受け、
リクエストごとに、その本文 (なければメソッドとパス) を本文とする 200 の応答を返す。`Connection: close` がなければ接続を続け
(パイプライン化したリクエストも受ける)、10 秒間なにも届かない接続は閉じる。引数なしでは空いているポートで待ち受け、自分に
リクエスト (パイプライン化したもの、分けて送るもの、壊れたもの、同時に 50 の接続) を送って応答を表示し、ループを止める (実行テスト)。

```
-- Serves the requests of one connection; `buf` holds the bytes received
-- and not yet answered.
serveConn : (1 s : TcpStream) -> String -> IO ()
serveConn s buf = case parse buf of
  Need => recvTimeout s 65536 10000 (\s, r => case r of
    Ok d => if d == "" then close s else serveConn s (buf ++ d)
    Err e => close s)
  Bad => sendAll s badRequest (\s, w => close s)
  Got req rest => sendAll s (response req) (\s, w => case w of
    Ok _ => if closes req then close s else serveConn s rest
    Err e => close s)

main : IO ()
main = runLoop (do
  r <- listen "127.0.0.1" 8080
  case r of
    Ok l => serve l (\s => serveConn s "")
    Err e => putStrLn ("listen failed: " ++ show e))
```

### 2.5 実行時の表現と実行、制限、診断

- `Std.Async` は `cirka_async_*` (このスレッドで動いているループに働くランタイムの関数) を、`Std.Net` は `cirka_net_*`・`cirka_sock_*` を
  `extern "cirka"` で呼ぶ (05-runtime.md §10.3)。ソケットはランタイムの記述子オブジェクト (`Handle`) で、解放されると記述子を閉じる。
- 継続をちょうど 1 回実行する約束はランタイムが守る。待ちを設定できなかったとき (閉じた記述子など) も、次のターンでエラーを渡して呼ぶ。
- MIR インタプリタはイベントループを扱わない。非同期 I/O の実行テストはネイティブだけで行う (`skip-backend: interp`)。
  `async.cirka` (スレッドプールと `Remote`) は TSan の構成でも実行する。
- v0.0.5 の制限:
  - 1 つのソケットで受け取りと送り出しを同時に待つこと (全二重の分割) はできない。ランタイムの待ちは記述子ごとに 1 つで、
    ソケットは線形なので一度に 1 つの操作にしか渡せない。
  - 複数の継続の合流 (N 個の接続が終わるのを待つ) は、`MVar` などで数を数えて書く (サンプルの `concurrent`)。
  - `stopLoop` で捨てた継続が持つ値はメモリを解放するが、`Drop`・`Release` の終了処理 (spec-v0.0.2.md §5.5) は実行しない。
    ソケットはランタイムの後始末で閉じる。
  - タスクの形 (継続を包む型に `Monad` を実装して `do` で書く。`tests/cirkalang/run/v005/async_task.cirka`) は、タイマーと動作なら書けるが、
    ソケットを扱えない。`Monad` の `>>=` は引数を ω で受け取り、結果は種 `Type` の値に限るので、線形なソケットを文から文へ渡せない
    (タスクに捕捉させると E0303・E0305 になり、結果には入れられない)。線形な bind を表すトレイトか効果 (v0.0.7) で書き味を比べる。
- 新しい診断コードはない。E1270 は `Handle`・関数・動作を `extern "C"` に渡したときにも出し、`extern "cirka"` では注に渡せる型を示す。

## 3. OS 層 (FEAT-0242)

syscall を使うプログラム (CLI ツール、サーバの起動と停止、子プロセスの管理) を書くためのライブラリ。libc とランタイムの関数
(05-runtime.md §11) を包み、失敗は v0.0.2 と同じく `Result IOError a` の値で返す。資源 (子プロセス・シグナルの受け口) は
線形な値で、使い終わりを型で追える。

### 3.1 errno と `Result` (`Std.Os`)

```
cResult : IO CInt -> IO (Result IOError Int)
cResultLong : IO CLong -> IO (Result IOError Int)
cResultSize : IO CSSize -> IO (Result IOError Int)
notPermitted, notFound, noSuchProcess, interrupted, badDescriptor, noChild, wouldBlock,
  permissionDenied, alreadyExists, notADirectory, isADirectory, invalidArgument, brokenPipe, notEmpty : IOError
```

- `cResult act` は、失敗すると -1 を返して `errno` を設定する C の関数 (`extern "C"` で宣言したもの) を実行し、-1 なら呼んだ直後の
  `errno` を `Err` に、それ以外の結果を `Ok` にする。`cResultLong` は `long` (`lseek` など)、`cResultSize` は `ssize_t` (`read`・`write`) の版。
  ライブラリが包まない syscall は、利用者が `extern "C"` で宣言してこれで `Result` にする (サンプルの `symlink`・`readlink`)。
- エラー番号の定数は Linux の値で、`==` で比べる (`if e == notFound then …`)。

### 3.2 子プロセス (`Std.Process`)

```
data Stdio = Inherit | Piped | Discard
data Command                                  -- プログラム、引数、標準入力・出力・エラーのつなぎ先
command : String -> List String -> Command
setStdin, setStdout, setStderr : Stdio -> Command -> Command
mergeErrors : Command -> Command              -- 2>&1
data ExitStatus = Exited Int | Signaled Int
succeeded : ExitStatus -> Bool
data Child : LType
spawn : Command -> IO1 (Result IOError Child)
childId : (1 c : Child) -> (Child, Int)
takeStdin, takeStdout, takeStderr : (1 c : Child) -> (Child, Maybe File)
wait : (1 c : Child) -> IO (Result IOError ExitStatus)
kill : (1 c : Child) -> Int -> IO1 (Child, Result IOError ())
run : Command -> IO (Result IOError ExitStatus)
data Output = MkOutput ExitStatus String String
output : Command -> String -> IO (Result IOError Output)
```

- `command prog args` は `prog` (`/` を含まなければ `PATH` から探す) を引数 `args` で実行するコマンドで、標準入力・出力・エラーは
  このプロセスのものを継承する。`Piped` はこのプロセスが読み書きするパイプ、`Discard` は `/dev/null` につなぐ。
  子は空のシグナルマスクと既定の `SIGPIPE` で始まる (このプロセスがブロック・無視していても、シェルから起動したのと同じ)。
- `Child` は **線形な値** で、`Release` が `wait` する。`wait` (と捨てること) は、取り出していないパイプを閉じてから子の終わりを待つ
  (入力を待っている子は終わりを見て終わる)。そのため子はかならず回収され (ゾンビが残らない)、`Child` がある間は子のプロセス id が
  ほかのプロセスに再利用されない。`kill` が別のプロセスへ届くことはない。
- パイプは `takeStdin` などで `Std.Fs.File` (線形な値。捨てると閉じる) として取り出し、`write`・`readAll` で読み書きする。
- `wait` はスレッドをブロックする。イベントループの上では `Std.Async.blocking (wait c) k` でスレッドプールに待たせる
  (`Child` と結果は `Send`)。
- `output cmd input` は、標準入力を `input`、出力とエラーをパイプにして実行し、すべて読んで終わりを待つ。入力を書くのと出力を読むのを
  同時に進めるので、入力を読み切る前に多く書く子でも止まらない。入力を読まずに終わった子への残りの入力は捨てる。
  `mergeErrors` なら、エラーは出力に入り `Output` のエラーは空になる。`run cmd` は継承したまま実行して終わりを待つ。
- プログラムが見つからなければ `spawn`・`run`・`output` は `Err notFound`。

### 3.3 シグナル (`Std.Os`)

```
sigHup, sigInt, sigQuit, sigKill, sigUsr1, sigUsr2, sigPipe, sigAlrm, sigTerm, sigChld, sigCont, sigStop, sigTstp, sigWinch : Int
ignoreSignal, defaultSignal : Int -> IO (Result IOError ())
raiseSignal : Int -> IO (Result IOError ())
sendSignal : Int -> Int -> IO (Result IOError ())        -- プロセス id、シグナル
blockSignals, unblockSignals : List Int -> IO (Result IOError ())
waitSignal : List Int -> Int -> IO (Maybe Int)
data Signals : LType
signals : List Int -> IO1 (Result IOError Signals)
nextSignal : (1 s : Signals) -> (1 k : (1 s : Signals) -> Result IOError Int -> IO ()) -> IO ()
closeSignals : (1 s : Signals) -> IO ()
```

- **シグナルハンドラは書けない。** シグナルはブロックして待つ (`waitSignal`) か、イベントループで受け取る (`signals`)。
  非同期のハンドラの中で使ってよい処理の制約 (async-signal-safe) を言語に持ち込まず、受け取った後はふつうのコードで扱う。
- `ignoreSignal`・`defaultSignal` はシグナルの動作を無視・既定にする。`ignoreSignal sigPipe` で、読む側のいないパイプへの書き込みは
  プログラムを終えず `Err brokenPipe` になる (既定では C と同じくプログラムが終わる。ソケットは v0.0.5 §2 のとおり常にエラー)。
  `sigKill`・`sigStop` と、ランタイムが使う `SIGSEGV`・`SIGBUS` は変えられない (`Err invalidArgument`)。
- `blockSignals` はこのスレッドでシグナルをブロックする (届いたシグナルは保留される)。`waitSignal sigs ms` は保留されたシグナルを
  1 つ取り出し、`ms` ミリ秒 (負なら無期限) 待っても来なければ `Nothing`。スレッドをブロックする。
- `signals sigs` はこのスレッドで `sigs` をブロックし、ループで読む受け口 `Signals` (線形な値。捨てると閉じる) を返す。`nextSignal s k` は
  次のシグナルを待って `k` に渡す (継続は v0.0.5 §2 と同じくちょうど 1 回実行される)。サーバが `SIGTERM` を受けて止まる処理などに使う。
- **スレッドとマスク.** プロセスへ送られたシグナルは、それをブロックしていないスレッドのどれかに届き、届いた先で既定の動作 (終了) が
  働きうる。ランタイムのスレッド (最初のスレッドとスレッドプール) はシグナルを受けないので、`Std.Thread` でスレッドを作らないプログラムは
  どこで `signals` を呼んでもよい。スレッドを作るプログラムは、スレッドを作る前に `blockSignals`・`signals` を呼ぶ
  (スレッドは作ったスレッドのマスクを継承する)。

### 3.4 ファイルとディレクトリ (`Std.Fs`・`Std.Env`)

```
listDirectory : String -> IO (Result IOError (List String))
data FileKind = RegularFile | Directory | SymbolicLink | OtherFile
data FileStatus
fileKind : FileStatus -> FileKind
fileSize, filePermissions, modifiedTime : FileStatus -> Int
fileStatus, linkStatus : String -> IO (Result IOError FileStatus)
makeDirectory, removeDirectory : String -> IO (Result IOError ())
rename : String -> String -> IO (Result IOError ())
currentDirectory : IO (Result IOError String)                    -- Std.Env
setCurrentDirectory : String -> IO (Result IOError ())           -- Std.Env
```

- `listDirectory` は `.`・`..` を除く名前を **並べ替えて** 返す (ディレクトリの順はファイルシステムで変わるので、結果を決まったものにする)。
- `fileStatus` はシンボリックリンクをたどり、`linkStatus` はたどらない。`filePermissions` は許可のビット (`0o755` = 493 など)、
  `modifiedTime` は 1970 年からの秒。`makeDirectory` の許可は `0o777` から umask を引いたもの。`removeDirectory` は空のディレクトリだけ。
- `setCurrentDirectory` はプロセスの (すべてのスレッドの) 作業ディレクトリを変える。

### 3.5 メモリマップとプロセスの情報 (`Std.Os`)

```
data Mapping
mapFile : String -> IO (Result IOError Mapping)
mappingSize : Mapping -> IO Int
mappingSlice : Mapping -> Int -> Int -> IO String              -- 位置、長さ
mappingByte : Mapping -> Int -> IO (Maybe Int)
mappingCount : Mapping -> Int -> Int -> Int -> IO Int          -- バイト、[from, to)
mappingFind : Mapping -> Int -> Int -> IO (Maybe Int)          -- バイト、from
parentProcessId, userId, effectiveUserId, groupId : IO Int
hostName : IO (Result IOError String)
isTerminal : Int -> IO Bool
```

- `mapFile` はファイルを読み取り専用で写像する。読む操作は文字列へコピーせずにファイルを読む (`mappingCount` で行を数えるなど)。
  `Mapping` は ω の値 (`Handle` を包む) で、最後の参照が消えると写像を外す。範囲は写像に切り詰める。
- 読む操作が `IO` なのは、写像中のファイルをほかのプロセスが変えると読む値が変わるからである。ほかのプロセスがファイルを短くすると、
  失われた部分を読んだときにプログラムが終わる (`SIGBUS`。C と同じ)。
- `isTerminal fd` は記述子 (0 標準入力、1 出力、2 エラー) が端末か。プロセス id は `Std.Process.processId`。

### 3.6 サンプル: OS 層を使う CLI ツール

`tests/cirkalang/run/v005/os_tool.cirka` は、`tree DIR…` (ファイルの種類と大きさ。リンクは `readlink` を `extern "C"` で呼んで先を示す)、
`lines FILE…` (写像で数えた行数)、`run PROG ARG…` (子の終わり方)、`signals` (ループで `SIGINT`・`SIGTERM` を待つ) のサブコマンドを持つ。
引数がなければ `/tmp` の新しいディレクトリで自分を試す: ディレクトリとファイルを作り、`symlink` を `cResult` で呼び (2 度目は
`Err alreadyExists`)、木と行数を表示し、`sh`・`tr`・`wc` を `output` で実行し、ないプログラム (`notFound`)、`SIGTERM` で終えた `sleep`
(`Signaled 15`)、パイプを 2 本取り出した `cat`、入力を読まずに終わった子への書き込み (`SIGPIPE` を無視して `brokenPipe`)、
`SIGUSR1` のブロックと待ち、スレッドプールで待つ子 (`Exited 7`) とプロセスへ送った `SIGUSR2` のループでの受け取り、改名と後片付けを行う。

### 3.7 実行時の表現と実行、制限

- `Std.Os`・`Std.Process`・`Std.Fs`・`Std.Env` は `cirka_os_*`・`cirka_fs_*` を `extern "cirka"` で呼ぶ (05-runtime.md §11)。
  `Signals` と `Mapping` はランタイムのオブジェクト (`Handle`) で、解放されると記述子を閉じ・写像を外す。`Child` はプロセス id と
  `Maybe File` 3 つを持つ線形なデータ型で、`Release` を実装する (spec-v0.0.2.md §5.5)。
- MIR インタプリタは OS 層を扱わない。実行テストはネイティブだけで行う (`skip-backend: interp`)。
- v0.0.5 の制限:
  - 子の終わりはイベントループで待てない (`blocking` でスレッドプールに待たせる)。`pidfd` をループで待つ形は必要になれば後の版で。
  - 子の環境変数と作業ディレクトリは変えられない (このプロセスのものを継承する)。
  - 書き込める写像 (`MAP_SHARED`)・無名の写像 (共有メモリ) はない。
  - シグナルは 1〜63 番。リアルタイムシグナルのキューの値 (`sigqueue`) は受け取らない。
- 新しい診断コードはない。
