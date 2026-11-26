# FEAT-1013: ストリームソケットの受信は `recv` で行う

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-1005, FEAT-0091
- 設計: [05-runtime.md](../../design/05-runtime.md) §10.2, [16-performance.md](../../design/16-performance.md)

## 概要

HTTP サーバの 1 要求あたりのシステムコールは Rust の版と同じ (受信 1・送信 1・`epoll_wait` 0.06〜0.07) だが、
カーネルの時間は Rust より約 2 % 多い (16 接続の静かな計測で 33.6 µs と 32.4〜32.9 µs)。ランタイムは TCP の受信に `read` を、
Rust は `recv` (`recvfrom` の宛先なし) を使う。ソケットの `read` はファイルの読み込みとして `rw_verify_area` (セキュリティ
モジュールのファイルの許可の検査。コンテナでは AppArmor のプロファイルがある)・`fsnotify`・読んだバイト数の会計を通ってから
`sock_recvmsg` に入る。`recv` は直接 `sock_recvmsg` に入る。`cirka_sock_recv` (TCP のストリームの受信) を `recv` にする。

## 作業内容

- [x] 設計: 05-runtime.md §10.2、16-performance.md の表
- [x] `cirka-runtime`: `cirka_sock_recv` を `recvfrom(fd, buf, n, 0, NULL, NULL)` で読む
- [x] テスト: イベントループの予算のテストはパイプでなくストリームソケットの組で読む (関数の仕様どおり)
- [x] 計測: HTTP サーバの 1 要求の CPU 時間 (ユーザー・カーネル)

## 受け入れ条件

- [x] HTTP サーバのカーネルの時間が増えず、出力 (応答) が変わらない
- [x] `scripts/ci.sh` が Docker コンテナ内で成功する

## メモ

- 2026-11-26: FEAT-1012〜1015 をまとめた版の HTTP サーバ (200,000 要求、5 回の交互実行の中央値、静かなホスト): 1 要求の
  カーネルの時間は 16 接続で 31.90 µs (Rust 32.35 µs)、64 接続で 32.50 µs (Rust 33.00 µs) と Rust より 1.4〜1.5 % 少ない
  (FEAT-1009 の版の計測では Rust より約 2 % 多かった)。システムコールの数は変わらない。応答は同じ。
