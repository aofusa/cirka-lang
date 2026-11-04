# FEAT-0021: 借用推論 (borrowed parameters)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2 Perceus
- 優先度: P1
- 依存: FEAT-0020
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

消費も保存もしない引数を borrowed にして dup/drop を減らす (Lean 4 方式)。

## 作業内容

- [x] 呼び出しグラフ上の不動点計算
- [x] 呼び出し側の dup/drop の調整

## 受け入れ条件

- [x] dup/drop 実行回数の削減を計測して記録
- [x] 検証器・リーク検査が通る
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装は `cirka-mir::borrow::infer_borrow`。`perceus::run` が 借用推論 → dup/drop 挿入 を行う (順序は Lean 4 と同じ。06-ir.md §3 を更新)。
- 引数を owned にする条件: 本体で消費される / その射影が消費される (再利用のため) / 末尾呼び出しで owned の値を受け取る。不動点に達するまで反復。
- 固定する関数: `main`、グローバル初期化関数、クロージャ関数の捕捉されない引数、手書きの dup/drop を持つ関数とその呼び出し先。
- 計測 (`mir_borrow_inference` テスト。全引数 owned にした入力での dup/drop 実行回数、推論なし → あり):
  list_map 604 → 204、list_sum 4001 → 2001、thunks 15 → 13、join_split 17 → 15、closures 30 → 29、その他は同数。
  arrays は `array_set` に一意なオブジェクトが必要なため、推論なしでは実行できない (推論ありで成功)。
