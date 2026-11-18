# FEAT-0089: 静的なオブジェクトの dup/drop の除去

- 状態: todo
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

- [ ] MIR で、値が静的なオブジェクトだと分かる変数 (引数のない構成子の `alloc`・文字列の定数、すべての入口で静的な値を受け取る結合点の引数) の `dup`/`drop` を除く
- [ ] 使われなくなった結合点の引数と、引数のない構成子の `alloc` を除く
- [ ] 単体テスト (MIR) と、`bench/effects` の 2 つの版の命令数の比較

## 受け入れ条件

- [ ] `bench/effects/direct.cirka` の命令数が `main.cirka` と同等になる
- [ ] ほかのベンチが遅くならない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
