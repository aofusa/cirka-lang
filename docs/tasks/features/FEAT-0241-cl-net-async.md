# FEAT-0241: ネットワークと非同期 I/O

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P0
- 依存: FEAT-0240, FEAT-0046
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

TCP/UDP ソケット (線形資源)、イベントループ上の非同期 API。

## 作業内容

- [ ] std.net
- [ ] 非同期 API の形 (コールバック / タスク) の試作
- [ ] タイムアウト

## 受け入れ条件

- [ ] HTTP エコーサーバのサンプル
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

