# FEAT-0124: 借用 (Ref / MutRef / Borrow モナド)

- 状態: todo
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0121
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

02 §5 の借用 API と構文糖。

## 作業内容

- [ ] `Ref`/`MutRef`/`Borrow r`/`borrowIn`
- [ ] 借用操作の消去
- [ ] 構文糖
- [ ] E0320

## 受け入れ条件

- [ ] クロージャ経由を含め借用の持ち出しが型エラーになる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

