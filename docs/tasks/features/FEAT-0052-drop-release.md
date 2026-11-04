# FEAT-0052: Drop / Release による RAII

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P0
- 依存: FEAT-0051
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [03-type-system.md](../../design/03-type-system.md)

## 概要

`Drop`・`Release` クラスと自動挿入 (Drop は未使用分岐の先頭、Release は do ブロック末尾、逆順)。

## 作業内容

- [ ] Weaken-Drop / Weaken-Release 規則と挿入位置の記録
- [ ] 脱糖での挿入
- [ ] Strict-Linear 規則 (03 §6)
- [ ] E0304 の診断

## 受け入れ条件

- [ ] 解放順序を出力で確認する run テスト
- [ ] Release のみの値を純粋文脈で捨てるとエラー
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

