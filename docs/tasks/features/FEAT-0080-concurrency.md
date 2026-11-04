# FEAT-0080: 並行性 (forkIO / MVar / アトミック RC)

- 状態: todo
- マイルストーン: M8 並行性
- 優先度: P2
- 依存: FEAT-0043
- 設計: [04-memory-model.md](../../design/04-memory-model.md), [05-runtime.md](../../design/05-runtime.md)

## 概要

OS スレッドによる `forkIO`、`MVar`、スレッド間共有時の共有マークとアトミック RC。

## 作業内容

- [ ] `cirka_mark_shared`
- [ ] 負の RC のアトミック操作
- [ ] forkIO/MVar/Chan
- [ ] ThreadSanitizer でのテスト

## 受け入れ条件

- [ ] 並行テストが TSan でエラーなし
- [ ] 単一スレッドの性能が低下しない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

