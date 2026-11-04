# FEAT-0057: 線形性の推論 (LType 一般化)

- 状態: todo
- マイルストーン: M5 線形性・一意性・RAII
- 優先度: P2
- 依存: FEAT-0051
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

型変数を LType に一般化できる場合の推論 (03 §1.2)。注釈の負担を減らす。

## 作業内容

- [ ] 型変数の使用量に基づく LType 一般化
- [ ] Prelude の関数 (`id`, `fst` の線形版など) の扱いの整理

## 受け入れ条件

- [ ] 注釈なしで `swap` 等が LType に使える
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

