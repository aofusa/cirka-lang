# FEAT-0112: 正格性解析と Strict/StrictData

- 状態: done
- トラック: aCHC
- マイルストーン: H2 Haskell 実用化 (スコープ縮小版)
- 優先度: P0
- 依存: FEAT-0033
- 設計: [04-memory-model.md](../../design/04-memory-model.md)、[06-ir.md](../../design/06-ir.md) §1.5

## 概要

需要解析による正格性解析 (aCHC の遅延評価のコスト削減)、`seq`/BangPatterns/`Strict`/`StrictData`。

## 作業内容

- [x] 需要解析 (FEAT-0041 の正格性解析 `ww::strictness` を使う。届く範囲を広げるため、辞書フィールドの大域化・局所関数のλ持ち上げ・辞書の自動特殊化を追加)
- [x] 正格な let・引数への変換 (`strict::strictify`) と float-in
- [x] `Strict`/`StrictData` (パーサ。`~T` の遅延フィールド、`~p` の遅延パターン)
- [x] bang パターンの `let`/`where` 束縛を正格に (それまでは無視されていた)
- [x] `undefined` の呼び出し位置を GHC と同じ `CallStack` で表示し、`error` の呼び出し位置の書き換えをライブラリの中では行わない

## 受け入れ条件

- [x] `sum [1..10^7]` 相当でサンクが作られないことを確保回数で確認
  (`tests/achc/run/h2/strictness.hs` の `max-allocs` / `max-forced`。累積値のサンクは 0。リストの尾のサンクは融合が要るので FEAT-0055)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 計測 (`CIRKA_STATS=1`、`-O2`、ネイティブ):
  - `sum [1 .. 10^7 :: Int]`: 確保 30,000,118 回、評価されたサンク 10^7 個 (リストの尾だけ。累積値は `foldl'` 相当で正格)。
  - `go acc n = if n == 0 then acc else go (acc + n) (n - 1)` (局所の `let`、10^7 回): 変更前は **スタックあふれ**、変更後は確保 0 回の `Int#` ループ。
  - 型シグネチャのない局所ループ (`Eq a, Num a` に一般化される) は特殊化で単相になる。ただし累積値が遅延したタプルに入るだけなら正格にならない (GHC も同じ) ので bang パターンが要る。
- 正格性解析は発散を全変数について正格とみなす (GHC -O と同じ、ADR-033)。差分テストの GHC は `-O0` だが、現在のテストで違いは出ていない。
- テストハーネスに `-- opt-levels:` / `-- max-allocs:` / `-- max-forced:` を追加 (10-testing.md)。ランタイムの `CIRKA_STATS` に `forced=` を追加。
- コンパイル時間 (デバッグビルドの achc): hello 1.63 → 1.55 秒、strictness.hs 3.37 → 3.01 秒 (不要な辞書コードが早く消えるため)。
