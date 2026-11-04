# FEAT-0051: 種 LType と Unique 型

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P0
- 依存: FEAT-0050
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

種 `Type ⊑ LType`、U-Bind 規則、データ型の種推論 (一意性の伝播)、`Unique`/`freeze`/`unique`、`Cirka.Ownership`。

## 作業内容

- [ ] 種システムへの LType 追加と部分種
- [ ] U-Bind・U-Arg 規則
- [ ] データ型の種の lub 推論、組み込み型の種多相
- [ ] `Consumable`/`Dupable`/`Movable`
- [ ] MIR で LType 値の dup がないことの検証

## 受け入れ条件

- [ ] E0301/E0310 のエラーテスト
- [ ] LType 値に対して生成コードに RC 操作がないことをスナップショットで確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

