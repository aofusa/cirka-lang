# FEAT-0002: Docker 開発環境と .env 読み込み

- 状態: done
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: -

## 概要

Dockerfile / compose.yaml で開発・ビルド・テストが完結することを確認し、`.env` を読む共通部分を用意する。

## 作業内容

- [x] `scripts/env.sh`: `.env` (なければ `.env.sample`) を読み込み `CIRKA_*` をエクスポート
- [x] `cirka-driver::config`: `CIRKA_CLANG` 等を環境変数から読む
- [x] README に Docker での使い方

## 受け入れ条件

- [x] `docker compose build` が成功し、コンテナ内で `cargo test` が通る
- [x] コンテナ内で作成したファイルの所有者がホストユーザー
- [x] `.env.sample` に全 `CIRKA_*` が説明付きで載っている
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 名前付きボリューム (`target-cache`, `cargo-registry`) が root 所有で作られ書き込めなかったため、Dockerfile でマウント先を開発ユーザー所有で作成しておく (新規ボリュームはイメージ側の所有者を引き継ぐ)。既存ボリュームは `docker compose down -v` で作り直す。
- `CIRKA_RUNTIME_DIR` を `.env.sample` に追加。
