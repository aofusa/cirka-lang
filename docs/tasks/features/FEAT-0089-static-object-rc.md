# FEAT-0089: 静的なオブジェクトの dup/drop の除去

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: BUG-0034
- 設計: [04-memory-model.md](../../design/04-memory-model.md), [16-performance.md](../../design/16-performance.md)

## 概要

引数のない構成子 (`()`・`True`・`Nothing` など) と文字列リテラルは静的なオブジェクト (参照数 0 の不死) で、`dup`/`drop` は何もしないが、
実行時に参照数を読んで比べる命令が残る。`do` の途中の `if c then act else pure ()` (BUG-0034 の修正の後) は、
各腕が `()` を作って結合点に渡し、結合点で `drop` する:

```
join3(res%10: ptr):
  drop res%10
  ...
alt4:
  let con%11 = alloc Prelude.Unit.Prelude.MkUnit()
  jump join3(con%11)
```

`bench/effects/direct.cirka` はこのために効果の版 (`main.cirka`) より 1 反復あたり 7 命令多い (callgrind、1000 万回で 330.2 M 対 260.2 M。時間は 669 ms 対 550 ms)。

## 作業内容

- [x] MIR で、値が静的なオブジェクトだと分かる変数 (引数のない構成子の `alloc`・文字列の定数、すべての入口で静的な値を受け取る結合点の引数) の `dup`/`drop` を除く
- [x] 使われなくなった結合点の引数と、引数のない構成子の `alloc` を除く
- [x] 単体テスト (MIR) と、`bench/effects` の 2 つの版の命令数の比較

## 受け入れ条件

- [x] `bench/effects/direct.cirka` の命令数が `main.cirka` と同等になる
- [x] ほかのベンチが遅くならない
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

## 結果

- `Function::static_vars` (`cirka-mir::ir`): 引数のない構成子の `alloc`・文字列定数を束縛する変数と、すべての `jump` からそれら
  (または文字列定数) だけを受け取るブロック引数 (ループの引数のため、静的と仮定してから外していく不動点)。
- Perceus (`insert_fn`) はそれらをスカラーとして扱い、`dup` / `drop` を挿入しない。その後、何も使わない静的なブロック引数と
  その実引数、使われない静的オブジェクトの定義を除く (`prune_static`)。手で RC を書いた関数は変えない。
- MIR 検証器は静的な変数を定数と同じく扱う (何度でも使え、消費しなくてよい。`dup` / `drop` は何もしない)。設計は ADR-105。
- テスト: `tests/mir/perceus/static_objects.mir` (結合点に `()` を渡す `do` の途中の `if`・文字列定数の結合点・
  `True` / `False` だけを持つループの引数・ヒープのオブジェクトも受け取る結合点 (`drop` が残る))。
  手で書いた MIR の実行テストから RC を除いて Perceus をかけ直す試験と、借用推論の試験も通る。
- `bench/effects` (callgrind、1000 万回。Docker): 名前で呼ぶ版 `direct.cirka` 330.2 M → 260.2 M、効果の版 `main.cirka` 260.2 M (変わらず)。
  同等になった。
- ほかのベンチ: `-O2` の MIR を比べると、array-sort・cli-grep・http-server の `main` だけが変わり (静的オブジェクトの `drop` と結合点の引数がなくなる)、
  aCHC の版はすべて同一。命令数 (callgrind、10 万) は array-sort 27 310 035 → 27 310 017、cli-grep 208 085 989 → 208 086 012
  (+23 命令。20 万行では 421 416 149 → 421 416 221 で +72 命令、0.00002 % で、LLVM の判断の差とみられる)。遅くなった題材はない。

