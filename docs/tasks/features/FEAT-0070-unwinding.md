# FEAT-0070: 例外のアンワインド

- 状態: todo
- マイルストーン: M7 互換性と性能
- 優先度: P1
- 依存: FEAT-0043, FEAT-0052
- 設計: [05-runtime.md](../../design/05-runtime.md), [07-backend.md](../../design/07-backend.md)

## 概要

Itanium ABI のアンワインドと landing pad での drop/release。Control.Exception の主要部分。

## 作業内容

- [ ] `invoke`/landing pad の生成
- [ ] `throwIO`/`throw`/`catch`/`bracket`/`finally`/`evaluate`
- [ ] 例外型の階層 (SomeException)
- [ ] W0305

## 受け入れ条件

- [ ] 例外発生時に Release が実行されることを出力で確認
- [ ] 例外経路でもリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

