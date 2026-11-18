# CirkaLang v0.0.7 言語仕様

- 版: v0.0.7 (プロトタイプ。版間の互換性は保証しない)
- チケット: FEAT-0260 (ケイパビリティ)、FEAT-0261 (効果型の試作)、FEAT-0262 (セッション型)、FEAT-0263 (リリース)、BUG-0033 (消去される値の全域性)
- 位置づけ: [12-cirkalang.md](../12-cirkalang.md) の「v0.0.7: 効果・能力・セッション型」を確定したもの。

v0.0.7 の目的は、**関数がどの種類の外部作用を起こしうるかを型に表すこと** である。
外の資源に名前で触れる操作と非決定的な値の観測は **能力** (capability) を要し、能力は `main` が受け取って必要な関数に渡す。
あわせて効果型とセッション型を試作する。
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
| §4 | 対象外 | FEAT-0260 |
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

## 4. 対象外

v0.0.7 で扱わないもの。

- 能力の弱め方 (特定のディレクトリだけの `FileSystem`、読み出しだけの `FileSystem` など) と取り消し。能力に実行時の表現が要る。
- パッケージ単位で `extern` ブロックを禁止する設定 (§1.5 の信頼の境界を、依存するパッケージについて強制すること)。
- 標準入出力・コマンドライン引数・スレッド・タイマーの能力 (§1.3。プログラムに渡されたもので、外の資源を名前で選ばない)。
- aCHC への取り込み (H7、FEAT-0160)。
