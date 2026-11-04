# FEAT-0043: IO・IORef・ハンドル・例外 (abort)

- 状態: todo
- マイルストーン: M4 Haskell 実用化
- 優先度: P0
- 依存: FEAT-0041
- 設計: [05-runtime.md](../../design/05-runtime.md)

## 概要

IO モナド (State# 消去)、System.IO の主要関数、IORef、`error`/`exitWith`/パターン失敗の abort 方式。

## 作業内容

- [ ] IO 型と State# の消去
- [ ] putStr/getLine/getContents/interact/readFile/writeFile/ハンドル
- [ ] IORef
- [ ] System.Exit・System.Environment
- [ ] `error` のメッセージ・終了コードを GHC と一致

## 受け入れ条件

- [ ] `.stdin` 付きの run テスト
- [ ] `error` 時の stderr と終了コードが GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

