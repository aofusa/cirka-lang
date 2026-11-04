# FEAT-0099: セッション型 (Cirka.Session)

- 状態: todo
- マイルストーン: M9 検証機能 (別軸・優先度低)
- 優先度: P2
- 依存: FEAT-0080, FEAT-0052
- 設計: [13-verification.md](../../design/13-verification.md)

## 概要

線形チャネルと GADTs によるセッション型ライブラリ。

## 作業内容

- [ ] `Send`/`Recv`/`End`/`Dual` と `send`/`recv`/`close`/`fork`
- [ ] 選択 (`Offer`/`Choose`)
- [ ] 例題 (クライアント/サーバ)

## 受け入れ条件

- [ ] プロトコル違反・途中放棄が型エラーになるテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

