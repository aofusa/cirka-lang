# FEAT-0002: Docker 開発環境と .env 読み込み

- 状態: todo
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: -

## 概要

Dockerfile / compose.yaml で開発・ビルド・テストが完結することを確認し、`.env` を読み込むスクリプトの共通部分を用意する。

## 作業内容

- [ ] `scripts/env.sh`: `.env` (なければ `.env.sample`) を読み込み `CIRKA_*` をエクスポート
- [ ] `achc` 側で `CIRKA_CLANG` 等を環境変数から読む設定モジュール (`cirka-driver::config`)
- [ ] README に Docker での使い方を記載

## 受け入れ条件

- [ ] `docker compose build` が成功し、コンテナ内で `cargo test` が通る
- [ ] コンテナ内で作成したファイルの所有者がホストユーザーになる
- [ ] `.env.sample` にすべての `CIRKA_*` 変数が説明付きで載っている
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

