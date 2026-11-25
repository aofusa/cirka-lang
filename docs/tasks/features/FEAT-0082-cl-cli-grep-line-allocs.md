# FEAT-0082: 行ごとの畳み込みの確保の除去 (cli-grep)

- 状態: in-progress
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: FEAT-0223
- 設計: [16-performance.md](../../design/16-performance.md) §4

## 概要

`bench/cli-grep` は Rust の 1.91 倍 (CirkaLang v0.0.3、bench-report.md)。100 万行で 200 万回の確保があり、
1 行あたり 2 回: `Data.String.foldLines` が渡す行のビュー `Str` と、`step` が返す累積値のレコード `MkCount` と考えられる。

## 作業内容

- [x] 確保の内訳を確かめる (最適化した Core と生成した LLVM IR。メモ)
- [x] 高階の畳み込み (`foldLines` / `foldWords`) を関数引数ごとに特殊化し、累積値と行のビューを worker/wrapper で unbox する
  (その後のチケットで済んでいた: `foldLinesFrom$f1$w$w` は累積値のフィールドと行の範囲を unboxed で受け取る)
- [x] simplify: `let! b = (case …)` と `case b of …` の間の全域で効果のないプリミティブ演算の `let!` を前へ出し、`let!` と `case` の連結
  (FEAT-0074) を適用する (06-ir.md §1.3)
- [x] 単体テストと実行テスト (確保の回数)

## 受け入れ条件

- [ ] `bench/cli-grep` の確保が 1 行あたり 1 回未満、Rust の 1.5 倍以内
- [ ] 全ベンチの出力が変わらず、時間が増えない
- [x] `scripts/ci.sh` が通る

## メモ

- 2026-11-25: 1 行のビュー `Str` はすでに確保しない (畳み込みが特殊化され、行は `(s, i, j)` で渡る)。残る確保は 1 行に 1 回の
  `MkCount` で、最適化した Core は `let! t = (case 一致 of 0 → MkCount …; _ → MkCount …) in let! u = i' + 1 in case t of MkCount … → 次の行`。
  次の行の位置 `u` の計算が `let! t` と `case t` の間にあるので、`let!` と `case` の連結 (FEAT-0074) が当たらず、両方の分岐で作った
  レコードを次の反復の wrapper が分解する (生成した機械語の行のループに `cirka_alloc(40)` と解放)。cli-grep は Rust の 1.02〜1.03 倍。
- 2026-11-25: 簡約の規則を入れて、行のループ (`foldLinesFrom$f1$w$w`) は確保しない (`MkCount` は最後に 1 回)。cli-grep の確保は
  引数 100000 で 500062 → 400062、出力は同じ。残る 1 行 4 回は入力の文章を作る `Builder` への `append` ごとの `MkBuilder` で、
  FEAT-1009 で扱う (`v007/fold_accumulator` の上限もそこで下げる)。
