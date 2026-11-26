# FEAT-1028: 演算ごとに動くランタイムの関数の稀な経路を関数の外に出し、受信を 1 つの関数にする

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1023, FEAT-1026, FEAT-0091
- 設計: [05-runtime.md](../../design/05-runtime.md) §3・§10.1・§10.2, [00-overview.md](../../design/00-overview.md) ADR-122, [16-performance.md](../../design/16-performance.md)

## 概要

HTTP サーバの 1 要求は、イベントループ・受信・送信・待ちの登録・解放のランタイムの関数を通る。どの関数も、熱い経路の命令の
間に、起こりにくい経路 (エラー・中断されたシステムコール・タイマー・他のスレッドからの受け箱・登録のやり直し) の命令が挟まって
いて、熱い経路だけで足りるより多くのキャッシュラインに触れていた (1 要求でランタイムのコードに約 33 ライン)。`cirka_free_object` は
オブジェクトの種類ごとのポインタのフィールドの数を `match` で求め、LLVM がそれを読み取り専用のデータの飛び先の表にしていた
(解放のたびにデータのラインを 1 本読む)。

- 稀な経路は `#[cold] #[inline(never)]` の関数に移す (`.text.unlikely.*` に置かれ、呼び出しの分岐も起こりにくくなる):
  - 受信: 何も読めなかった読み込み (ストリームの終わり・エラー・`EAGAIN`・中断) の後始末 (`read_failed`)
  - 送信: 閉じたハンドル・予算切れ・満杯とわかっているソケット (`send_not_now`)、失敗した送信と範囲外のオフセット (`send_failed`)
  - 待ちの登録 (`cirka_async_watch`): 速い経路 (`watch_fast`) で登録できないもの (`async_watch_slow`)
  - イベントループ: 空でない受け箱の実行 (`run_inbox_items`)・受け箱の状態をロックの中で読む (`inbox_state_locked`)・受け箱の
    eventfd のイベント (`inbox_woken`)・タイムアウト付きの待ちのタイマーの解放 (`drop_timer`)・タイマーのある待ちの時間
    (`timer_timeout_ms`)・`epoll_wait` の失敗 (`wait_failed`)
- 受信は 1 つの関数にする: `cirka_sock_recv` (と `cirka_sock_recvfrom`) にハンドルの検査から読み込み・新しい BYTES オブジェクトの
  確保とコピーまでをインライン化する (前は `cirka_sock_recv` → `read_now` → `cirka_bytes_from` の 3 つの関数)。
- 16 から 64 バイトのコピー (`copy_bytes`、FEAT-1022) は 1 つの経路にする (先頭と末尾の 16 バイト、32 バイトを超えたら 2 番目と
  最後から 2 番目の 16 バイト)。受信したバイトの文字列の作成が分岐 1 つで済む。
- `cirka_free_object` のポインタのフィールドがないオブジェクトの判定は、種類のビットの定数との論理積にする (`no_ptr_fields`)。
  飛び先の表を読まない。

## 作業内容

- [x] 設計: 05-runtime.md §3・§10.1・§10.2、00-overview.md の ADR-122、16-performance.md の表
- [x] `cirka-runtime`: `net.rs` (`read_failed`・`send_not_now`・`send_failed`、`read_now` のインライン化)、`lib.rs`
  (`bytes_from`・`copy_bytes`)、`evloop.rs` (`watch_fast`・`async_watch_slow` と、ループの稀な経路の関数)、`object.rs` (`no_ptr_fields`)
- [x] テスト: `no_ptr_fields` が `ptr_fields` と同じ判定であること (ユニットテスト)。既存の受信・送信・ループ・コピーのテストと実行テスト
- [x] 計測: HTTP サーバの 1 要求の命令数・キャッシュライン・CPU 時間、binary-trees、全ベンチ

## 受け入れ条件

- [x] 全ベンチの出力が変わらず、遅くなるベンチがない
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- binary-trees (Docker、5 回の中央値): FEAT-1027 17.74 秒 → 16.78 秒 (0.946)。命令数は同じ (飛び先の表の読み出しがなくなった)。
- HTTP サーバの 1 要求の命令数: 872 → 853。ランタイムのコードのライン (lackey): 送信 5.0 → 4.0、待ちの登録 4.1 → 3.4、
  ループ 5.1 → 4.3、受信の読み込み 4.0 → 3.0、文字列の作成 4.0 → 3.0。
- 受信を 1 つの関数にした後 (Docker、FEAT-1026 と Rust と交互の 9 回の中央値): HTTP サーバの 1 要求の命令数 853 → 838、
  コードのキャッシュライン 75.0 → 73.2 本。スループットは FEAT-1026 の 1.010 倍 (16 接続)・1.012 倍 (64 接続) で、Rust に対して
  0.999 倍・1.003 倍 (ユーザー空間の時間 3.75 µs と Rust の 3.40 µs、カーネルの時間 32.15 µs と Rust の 32.55 µs)。
  binary-trees は変わらない (FEAT-1026 の 0.751 倍)。
- 採らなかったもの: LLVM の機械関数の分割 (`-fsplit-machine-functions`、生成コードの関数に入口の回数 1000 を付けて重みの小さい
  ブロックを `.text.unlikely` へ): HTTP サーバのラインは減らず (77.3 本)、binary-trees が 1.15 倍遅くなった (熱いブロックが冷たい節に
  移る)。`drop` の重みを解放が起こりやすい向きにする: binary-trees が 2 % 遅くなった。
- 全ベンチは最終の計測 (FEAT-0091 のメモ)。
