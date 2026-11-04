# FEAT-0120: LinearTypes (多重度)

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0110
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [03-type-system.md](../../design/03-type-system.md)

## 概要

GHC LinearTypes 互換の多重度と `%0`、let の多重度推論、Core Lint の多重度検査。

## 作業内容

- [ ] 多重度の構文と型
- [ ] 使用量の検査
- [ ] let の多重度推論
- [ ] `Prelude.Linear` 最小サブセット

## 受け入れ条件

- [ ] linear-base の README 程度の例が通る
- [ ] E0302/E0303 のエラーテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

