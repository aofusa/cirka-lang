# FEAT-0241: ネットワークと非同期 I/O

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P0
- 依存: FEAT-0240, FEAT-0046
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

TCP/UDP ソケット (線形資源)、イベントループ上の非同期 API。

## 作業内容

- [x] std.net (`lib/cirkalang/std/Std/Net.cirka`: TCP の待ち受け・接続・送受信・名前解決、UDP。ソケットは線形な値)
- [x] 非同期 API の形 (コールバック / タスク) の試作 (`Std.Async` の継続渡しを採用。タスクの形は `async_task.cirka` で試作)
- [x] タイムアウト (`acceptTimeout`・`connectTimeout`・`recvTimeout`・`sendAllTimeout`・`recvFromTimeout`、ランタイムの待ちのタイムアウト)

## 受け入れ条件

- [x] HTTP エコーサーバのサンプル (`tests/cirkalang/run/v005/http_echo.cirka`、spec-v0.0.5.md §2.4)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

**API の形 (ADR-089、spec-v0.0.5.md §2).** 待つ操作は、残りの処理を一度だけ実行される継続 `(1 k : …)` として受け取ってすぐに戻る。
ソケットは線形な値 (`TcpListener`・`TcpStream`・`UdpSocket : LType`) で、操作はソケットを受け取って継続に返す。
同じソケットを 2 つの操作に渡すと E0303、何度も実行される関数 (`serve` の `handler`) に捕捉させると E0305 になる (`tests/cirkalang/fail/net/`)。
ソケットはランタイムの記述子オブジェクト (`Handle`) を包み、捨てれば解放のときに閉じる。`stopLoop` で捨てた継続が持つソケットも閉じる。

**タスクの形の試作.** 継続を包む `Task a` に `Monad` を実装すると、タイマーと動作は `do` で書ける (`tests/cirkalang/run/v005/async_task.cirka`)。
ただし `Monad` の `>>=` は引数を ω で受け取り、結果は種 `Type` に限るので、線形なソケットを文から文へ渡せない
(タスクに捕捉させると E0303・E0305、結果の組には入れられない)。タスクに要るのは `IO1` の `primBind1` にあたる線形な bind で、
効果 (v0.0.7) と重なるので、そこで書き味を比べる。v0.0.5 のライブラリは継続を受け取る形にした。

**ランタイム (05-runtime.md §10.3).** ライブラリが使う `cirka_async_*` は、このスレッドで動いているループに働く (ループをオブジェクトとして渡さない)。
加えたもの: 待ちのタイムアウト (待ちにタイマーを付け、期限で `EV_TIMEOUT` = 8)、設定できない待ちも次のターンでエラーを渡して呼ぶこと
(継続がちょうど 1 回実行される)、コールバックごとの予算 (すぐに完了するソケットの操作は 64 回まで。それを超えると `EAGAIN` で待ちに入る)、
記述子オブジェクト (`cirka_fd_handle`・`cirka_handle_fd`・`cirka_handle_close`、解放で閉じる)、それを取るソケットの操作 (`cirka_sock_*`)、UDP。

**コンパイラ.** `Foreign` に `prim Handle : Type` (資源を持たない ω の型。`Send`)。`extern "cirka"` は `Handle` と関数・動作を取れる
(クロージャを `Any` として借用で渡し、ランタイムが持つときは参照を増やす)。`extern "C"` には渡せない (E1270。ランタイムの規約では注で渡せる型を示す)。

**見つけたバグ.** `http_echo.cirka` のコンパイルが終わらなかった (最適化後の Core が 14 MB)。インライン化が wrapper 経由で 1 回だけ呼ばれる worker を
指数的に複製していた。BUG-0021 として別に直した。

**決めたこと・v0.0.5 の制限 (spec-v0.0.5.md §2.5).**

- 1 つのソケットで受け取りと送り出しを同時に待つこと (全二重の分割) はしない。ランタイムの待ちは記述子ごとに 1 つで、ソケットは線形なので
  一度に 1 つの操作にしか渡せない。HTTP/1.1 のサーバ (応答を送り終えてから次を読む) には要らない。
- 複数の継続の合流は `MVar` で数えて書く (`http_echo.cirka` の `concurrent`)。
- `stopLoop` で捨てた継続が持つ値のうち、`Drop`・`Release` の終了処理を持つものは終了処理を実行しない (メモリは解放する)。
  ソケットはランタイムの後始末で閉じる。
- 予算 64 は libuv の読み取りの上限 (1 回の読み取りの事象で 32 回) と同じ程度の値。性能は FEAT-0243 の HTTP のベンチで確かめる。

**テスト.** ランタイムの単体テスト 5 つ (タイムアウト付きの待ち、設定できない待ち、予算、記述子オブジェクト、データグラム)。
CirkaLang の実行テスト `tests/cirkalang/run/v005/` の `async` (TSan の構成でも)・`async_task`・`net_tcp` (1 つのループで 100 のクライアント、
1 MB の送受信、タイムアウト、エラー、名前解決、`stopLoop` で閉じるソケット)・`net_udp`・`http_echo` (パイプライン化、分けて届くリクエスト、
壊れたリクエスト、同時に 50 の接続)。診断のテスト `tests/cirkalang/fail/net/` (`extern "C"` の `Handle` と関数、ソケットの 2 度の使用と捕捉)。
