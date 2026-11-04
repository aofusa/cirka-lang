# FEAT-0021: 借用推論 (borrowed parameters)

- 状態: todo
- マイルストーン: M2 Perceus
- 優先度: P1
- 依存: FEAT-0020
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

消費も保存もしない引数を borrowed にして dup/drop を減らす (Lean 4 方式)。

## 作業内容

- [ ] 関数呼び出しグラフ上の不動点計算
- [ ] 呼び出し側の dup/drop の調整

## 受け入れ条件

- [ ] ベンチで dup/drop 実行回数が減ったことを計測して記録
- [ ] 検証器・リーク検査が通る
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

