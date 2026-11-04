# FEAT-0022: drop 特殊化と再利用解析 (FBIP)

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2 Perceus
- 優先度: P0
- 依存: FEAT-0020
- 設計: [04-memory-model.md](../../design/04-memory-model.md)

## 概要

drop 特殊化、`drop_reuse` による同サイズセルの再利用、dup/drop 融合。

## 作業内容

- [x] drop 特殊化
- [x] 再利用解析
- [x] dup/drop 融合

## 受け入れ条件

- [x] RC==1 のリストの `map` が新規確保なしで動く
- [x] M1 のベンチが手書き MIR と同等
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `cirka-mir::reuse::optimize` (融合 → 再利用 → フィールドの dup を親の drop 直前へ移動) を `perceus::run` の最後に実行する。
- drop 特殊化は MIR に新しい命令を足さず、LLVM バックエンドの覗き穴最適化として実装 (`dup y..; drop x` / `dup y..; drop_reuse x` → 一意性テスト)。07-backend.md §2 に追記。
- Perceus は「どこかで消費される射影」を射影直後に dup する (Koka の順序) よう変更。これにより親が早く死に、`map` の再帰先でも RC==1 が保たれる。
- テスト: `fbip_map_reuses_unique_cells` (一意なリストの map で 1000 セルすべて再利用、共有リストではコピー)、`tests/mir/perceus/map_reuse` のネイティブ実行 (debug/release ランタイム) と確保/解放回数のインタプリタとの一致。
- ベンチ: `bench/binary-trees/mir-perceus/` (dup/drop なしの MIR + `--perceus`) は手書き MIR と同一の MIR を生成し、深さ 19 で 3.36 s vs 3.36 s。
