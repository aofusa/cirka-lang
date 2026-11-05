# FEAT-0122: Drop / Release による RAII

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P0
- 依存: FEAT-0121
- 設計: [03-type-system.md](../../design/03-type-system.md)

## 概要

`Drop`・`Release` と自動挿入、Strict-Linear 規則。

## 作業内容

- [x] 挿入位置の記録と挿入 (型検査の最後のパス `raii.rs`、ADR-039)
- [x] Strict-Linear 規則
- [x] E0304

## 受け入れ条件

- [x] 解放順序を出力で確認する run テスト (`tests/achc/run/h3/raii.hs`)
- [x] Release のみの値を純粋文脈で捨てるとエラー (`tests/achc/typeck/fail/raii.hs`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `drop` の挿入は MIR のスナップショット (`tests/achc/mir/drop.hs`) で確認する (純粋な `Drop` は出力で観測できない)。
- 例外経路での解放 (W0305) は未対応 (前期の `error` はプログラムを終了する)。
- `Drop a` を文脈に持つ多相関数での自動挿入は未対応 (閉じた型だけ)。

