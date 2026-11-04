# FEAT-0016: M1 ベースライン性能検証

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P1
- 依存: FEAT-0015, FEAT-0011, FEAT-0012
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

手書き MIR の binary-trees 等を Rust と比較し、RC + 本ランタイムの基礎性能を確認する (最重要リスクの早期検証)。

## 作業内容

- [ ] `bench/binary-trees/mir/` と `rust/`
- [ ] `scripts/bench.sh`
- [ ] 結果を `docs/design/bench-report.md` に記録

## 受け入れ条件

- [ ] Rust の 2 倍以内 (超えた場合は原因分析と対策チケットの起票)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

