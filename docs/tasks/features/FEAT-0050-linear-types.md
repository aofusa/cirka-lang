# FEAT-0050: LinearTypes (多重度)

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P0
- 依存: FEAT-0040
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [03-type-system.md](../../design/03-type-system.md)

## 概要

GHC LinearTypes 互換の多重度 (`%1`, `%m`, `Ur`) と aCHC 追加の `%0`、let の多重度推論、Core Lint の多重度検査。

## 作業内容

- [ ] 型・構文への多重度の追加
- [ ] 使用量の検査 (03 §2)
- [ ] let の多重度推論
- [ ] Core Lint の多重度検査
- [ ] `Prelude.Linear` 最小サブセット

## 受け入れ条件

- [ ] linear-base の README 程度の例が通る
- [ ] E0302/E0303 のエラーテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

