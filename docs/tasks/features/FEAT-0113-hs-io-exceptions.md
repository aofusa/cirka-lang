# FEAT-0113: IO・IORef・ハンドル・例外

- 状態: todo
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0111
- 設計: [05-runtime.md](../../design/05-runtime.md)

## 概要

IO (State# 消去)、System.IO 主要関数、IORef、`error`/`exitWith` (abort)、`catch`/`bracket` は 0040 の後で。

## 作業内容

- [ ] IO 型と State# の消去
- [ ] putStr/getLine/getContents/interact/readFile/writeFile
- [ ] IORef
- [ ] System.Exit / Environment
- [ ] `error` のメッセージ・終了コードを GHC と一致

## 受け入れ条件

- [ ] `.stdin` 付き run テスト
- [ ] `error` 時の stderr と終了コードが GHC と一致
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

