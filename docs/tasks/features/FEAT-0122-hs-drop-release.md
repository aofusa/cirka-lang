# FEAT-0122: Drop / Release による RAII

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0121
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

`Drop`・`Release` と自動挿入、Strict-Linear 規則。

## 作業内容

- [ ] 挿入位置の記録と脱糖での挿入
- [ ] Strict-Linear 規則
- [ ] E0304

## 受け入れ条件

- [ ] 解放順序を出力で確認する run テスト
- [ ] Release のみの値を純粋文脈で捨てるとエラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

