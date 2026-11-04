# FEAT-0055: リージョン

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P2
- 依存: FEAT-0054
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [04-memory-model.md](../../design/04-memory-model.md)

## 概要

`withRegion`/`RIO r`/`allocIn`/`readIn`/`newRefIn` とアリーナアロケータ。

## 作業内容

- [ ] アリーナ (ページ単位のバンプアロケータ)
- [ ] 外部参照リスト
- [ ] 構文糖 `region r in`

## 受け入れ条件

- [ ] リージョン内で循環グラフを作り、終了時にリーク 0
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

