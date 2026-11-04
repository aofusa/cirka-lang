# FEAT-0054: 借用 (Ref / MutRef / Borrow モナド)

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P1
- 依存: FEAT-0051
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

02 §5 の借用 API と `borrow ... as ... in` 構文糖。

## 作業内容

- [ ] `Ref`/`MutRef`/`Borrow r`/`borrowIn`
- [ ] 借用操作の消去 (RC 操作なし)
- [ ] 構文糖
- [ ] 脱出時のエラーメッセージ (E0320) の言い換え

## 受け入れ条件

- [ ] 借用を持ち出そうとするコード (クロージャ経由を含む) が型エラーになるテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

