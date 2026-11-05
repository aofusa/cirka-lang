# FEAT-0057: IO の結果の unboxing (bind を通る値の箱を消す)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: BUG-0011
- 設計: docs/design/06-ir.md §1.6

## 概要

BUG-0011 の修正で、IO (と `Borrow` などの newtype) のループは状態トークンを受け渡す直接の呼び出しになった。
しかし `x <- U.index a i` のように bind を通る `Double` / `Int` は箱 (`D#`) のまま受け渡され、
n-body の借用版は 1 ステップあたり約 190 回確保する (線形版は 0 回)。20 万ステップで 0.91 s 対 0.13 s。

## 作業内容

- [ ] `f$io` の結果に CPR (worker/wrapper の構築子結果) を適用する。結果は `a` で、状態トークンの後ろに来る
- [ ] bind の継続に渡る値が即座に `case` される場合に、箱を作らない (`let! x = … in case x of D# u`)
- [ ] 計測して 06-ir.md §1.6 と bench-report を更新

## 受け入れ条件

- [ ] n-body の借用版の確保がステップ数に比例しない (または大幅に減る)
- [ ] `tests/achc/run/h3/borrow_loop.hs` の `max-allocs` を下げる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
