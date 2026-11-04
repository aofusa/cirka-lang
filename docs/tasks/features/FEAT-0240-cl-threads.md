# FEAT-0240: スレッドと共有

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.5 並行性・サーバ・syscall
- 優先度: P0
- 依存: FEAT-0235, FEAT-0045
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

スレッド生成、`Send`/`Sync` 相当のトレイト (スレッド間で渡せる型の区別)、Mutex・チャネル。

## 作業内容

- [ ] スレッド API
- [ ] スレッド安全性のトレイトと自動導出
- [ ] 同期プリミティブ

## 受け入れ条件

- [ ] データ競合しない並行テスト (TSan)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

