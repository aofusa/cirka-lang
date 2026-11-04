# FEAT-0052: 遠隔検証スクリプト (scripts/remote.sh)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0002
- 設計: [15-gpu-and-remote.md](../../design/15-gpu-and-remote.md)

## 概要

`.env` の接続先 (`CIRKA_REMOTE_*`) を読み、遠隔ホストの作業ディレクトリへの同期・ビルド・実行・結果回収を行う。接続先の実際の値は git に入れない。

## 作業内容

- [ ] check / sync / build / run / fetch
- [ ] macOS は rsync、Windows は scp + tar
- [ ] 変数が空ならスキップ
- [ ] 作業ディレクトリ外を変更しない安全策

## 受け入れ条件

- [ ] 両ホストで check と sync が動く (利用者の環境で確認)
- [ ] リポジトリ内に実際のホスト名・パスが含まれないことを確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

