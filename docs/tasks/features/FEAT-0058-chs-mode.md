# FEAT-0058: .chs モード・CirkaOwnership・ownership レベル

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P1
- 依存: FEAT-0052
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

.chs 拡張子と `CirkaOwnership` pragma、`achc.toml` の `ownership = off|warn|strict`、診断の改善。

## 作業内容

- [ ] 拡張子によるモード切替と既定拡張
- [ ] .hs と .chs の相互 import
- [ ] ownership レベルによる診断の重大度切替
- [ ] 主要エラーに help を付ける

## 受け入れ条件

- [ ] .hs と .chs が混在するプロジェクトの run テスト
- [ ] 診断のスナップショット
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

