# FEAT-0016: M1 ベースライン性能検証

- 状態: done
- トラック: 共通基盤
- マイルストーン: M1 バックエンド基盤
- 優先度: P1
- 依存: FEAT-0015, FEAT-0011, FEAT-0012
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

手書き MIR の binary-trees 等を Rust と比較し、RC + 本ランタイムの基礎性能を確認する (最重要リスクの早期検証)。

## 作業内容

- [x] `bench/binary-trees/mir/` と `rust/`
- [x] `scripts/bench.sh`
- [x] 結果を `docs/design/bench-report.md` に記録

## 受け入れ条件

- [x] Rust の 2 倍以内 (超えた場合は原因分析と対策チケットの起票)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `bench/run.sh` (`scripts/bench.sh` から呼ぶ) が `bench/<name>/` 内の実装 (mir / cirka / chs / hs / rust) を見つけてビルドし、hyperfine で比較する。引数は `bench/<name>/args`。
- 深さ 21: MIR 15.59 s、Rust (`Option<Box>`) 23.10 s。Rust 比 0.67 倍で条件を満たす。詳細は `docs/design/bench-report.md`。
- テスト: `tests/mir.rs` の `bench_binary_trees_mir` がベンチの MIR を検証し、インタプリタ (引数 6) でリーク 0・期待出力を確認する。そのためインタプリタに `cirka_argc` / `cirka_arg` を追加した。
