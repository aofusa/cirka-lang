# FEAT-0002: Docker 開発環境と .env 読み込み

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: -

## 概要

Dockerfile / compose.yaml で開発・ビルド・テストが完結することを確認し、`.env` を読む共通部分を用意する。

## 作業内容

- [ ] `scripts/env.sh`: `.env` (なければ `.env.sample`) を読み込み `CIRKA_*` をエクスポート
- [ ] `cirka-driver::config`: `CIRKA_CLANG` 等を環境変数から読む
- [ ] README に Docker での使い方

## 受け入れ条件

- [ ] `docker compose build` が成功し、コンテナ内で `cargo test` が通る
- [ ] コンテナ内で作成したファイルの所有者がホストユーザー
- [ ] `.env.sample` に全 `CIRKA_*` が説明付きで載っている
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

