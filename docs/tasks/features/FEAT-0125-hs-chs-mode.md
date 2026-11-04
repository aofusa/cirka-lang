# FEAT-0125: .chs モード・CirkaOwnership・ownership レベル

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0122
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

.chs 拡張子と pragma、`ownership = off|warn|strict`、診断の改善。

## 作業内容

- [ ] モード切替と既定拡張
- [ ] .hs と .chs の相互 import
- [ ] 診断の重大度切替

## 受け入れ条件

- [ ] .hs と .chs が混在するプロジェクトの run テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

