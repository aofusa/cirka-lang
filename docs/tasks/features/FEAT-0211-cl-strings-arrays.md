# FEAT-0211: 文字列と配列

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P0
- 依存: FEAT-0210, FEAT-0022
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

UTF-8 の所有文字列 `String` と借用 `Str`、一意配列 `Array` / 可変長 `Vec` の in-place 操作。

## 作業内容

- [ ] String / Str
- [ ] Array / Vec (Unique で in-place)
- [ ] 境界検査 (v0.0.4 で証明により除去可能にする)

## 受け入れ条件

- [ ] 文字列処理・配列ソートのベンチを記録
- [ ] 書き込みで確保が発生しない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

