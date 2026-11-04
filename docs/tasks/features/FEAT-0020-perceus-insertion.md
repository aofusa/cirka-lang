# FEAT-0020: Perceus: dup/drop の自動挿入

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0011
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

dup/drop を含まない MIR から精密な dup/drop を挿入する。

## 作業内容

- [x] 生存変数解析
- [x] 最後の使用での移動・分岐先頭での drop・フィールド読み出し時の dup
- [x] 挿入後に検証器を実行

## 受け入れ条件

- [x] `tests/mir/perceus/` のスナップショット
- [x] 挿入後の全テストがリーク 0
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装は `cirka-mir::perceus` (`insert_rc`)。入力は dup/drop を含まない MIR。既に dup/drop/drop_reuse を含む関数は手書きとみなして変更しない。
- `proj` / `array_get` の結果は借用 (Lean 4 の borrowed projection) とし、その使用を親の (借用) 使用として生存解析に含める。親はフィールドの最後の使用まで生き、フィールドを消費する場合は `dup` する。dup/drop の打ち消しは FEAT-0022 の drop 特殊化で行う。
- `switch` の辺で死ぬ変数は分岐先の先頭で drop する。分岐先に他の先行ブロックがある場合は辺を分割する (`<label>.eN`)。
- 借用引数に所有値を渡す `tailcall` は、呼び出し後に drop が必要なので `call` + `drop` + `ret` に変換する。
- テスト: `tests/mir/perceus/` (スナップショット `.out` + 実行・リーク 0)、`mir_perceus_roundtrip` (`tests/mir/run` の手書き dup/drop を除去 → 再挿入 → 検証器・インタプリタでリーク 0)。
