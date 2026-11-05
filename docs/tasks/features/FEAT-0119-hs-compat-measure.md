# FEAT-0119: 互換性の計測 (GHC testsuite / nofib の一部)

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P2
- 依存: FEAT-0117
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

合格率の向上は目標にせず、「GC なし実行で何が問題になるか」を知るための計測に限る。

## 作業内容

- [x] 取り込みスクリプト (ライセンス表記保持): `scripts/import-compat.py`
- [x] 失敗理由の分類: `tests/achc_compat.rs` (`scripts/compat.sh`)
- [x] `docs/design/compat-report.md`

## 受け入れ条件

- [x] 失敗理由の分類レポートが CirkaLang 設計へのフィードバックとして記録されている (compat-report.md §3, §4)
- [x] テストを追加し `scripts/ci.sh` が通る (分類器の単体テスト。計測本体は `scripts/compat.sh`)

## メモ

- 計測 (2026-11-05): GHC testsuite 321 本中 pass 104、nofib 47 本中 pass 19。リーク 0。詳細は compat-report.md。
- nofib はライセンスが一様でない (表記のないプログラムがある) ため、AGENTS.md §8 のライセンス条件に合わせてコミットしない。
- 計測で見つけたもの: BUG-0006 (修正)、BUG-0007、BUG-0008 (起票)、引数のない型クラスでのパニック (FEAT-0118 の退行、修正)、
  `scaleFloat` の巨大な指数での停止 (GHC と同じく指数を制限して修正、`run/h2/numeric.hs` に回帰テスト)、
  `Rank2Types` を `RankNTypes` の別名として受理。
- コンパイラの異常終了 (スタックオーバーフロー) で計測全体が止まったため、各プログラムを子プロセスで計測する。
