# FEAT-0121: 種 LType と Unique 型

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0120
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

`Type ⊑ LType`、U-Bind、データ型の種推論、`Unique`/`freeze`/`unique`、`Cirka.Ownership`。

## 作業内容

- [ ] LType と部分種
- [ ] U-Bind・U-Arg
- [ ] データ型の種の lub 推論
- [ ] `Consumable`/`Dupable`/`Movable`

## 受け入れ条件

- [ ] E0301/E0310 のエラーテスト
- [ ] LType 値に RC 操作がないことをスナップショットで確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

