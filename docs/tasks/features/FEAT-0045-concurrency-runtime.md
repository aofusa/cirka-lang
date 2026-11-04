# FEAT-0045: 並行性ランタイム (スレッド・アトミック RC)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0031
- 設計: [04-memory-model.md](../../design/04-memory-model.md), [05-runtime.md](../../design/05-runtime.md)

## 概要

OS スレッド、スレッド間共有時の共有マークとアトミック RC、同期プリミティブ。

## 作業内容

- [ ] `cirka_mark_shared`
- [ ] 負の RC のアトミック操作
- [ ] スレッド生成・join・Mutex/条件変数/チャネルのランタイム部
- [ ] ThreadSanitizer でのテスト

## 受け入れ条件

- [ ] 並行テストが TSan でエラーなし
- [ ] 単一スレッドの性能が低下しない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

