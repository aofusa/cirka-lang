# FEAT-0090: 線形な値を返す case の case-of-case

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P2
- 依存: FEAT-0262
- 設計: [06-ir.md](../../design/06-ir.md), [16-performance.md](../../design/16-performance.md)

## 概要

分岐ごとに構成子を作って返す式をすぐに `case` で分解するとき、共通 Core の case-of-case は外側の `case` を内側の各分岐に入れ、
構成子を作らずに済ませる (FEAT-0041)。次の場合はこれが起きず、構成子が確保される:

- 外側の `case` の束縛子 (分解する値) が線形 (`LType` の型の値) のとき。`case_of_case_ok` は束縛子が `Many` でなければ断る。
- 内側の分岐の結果が `let t = K .. in K2 t` の形のとき (構成子と分からない)。
- 内側の `case` と外側の `case` の間に、自分の型への `cast` があるとき (`IO1` の結果)。

`Std.Session` の `offer` (spec-v0.0.7.md §3) は `if b then Left (MkChannel o i) else Right (MkChannel o i)` を返し、
呼び出し側がすぐに `case` で分解するが、`bench/session/main.cirka` では受け取る側が 1 往復ごとに `MkChannel` と `Branch` の 2 つを確保する
(`direct.cirka` は 1 往復 2 つ、`main.cirka` は 4 つ)。

## 作業内容

- [ ] 内側の結果がすべて (`let` で包まれた) 構成子なら、線形な束縛子・フィールドでも case-of-case をする (各写しは既知の構成子の `case` としてすぐに簡約される)
- [ ] 自分の型への `cast` を除く
- [ ] 単体テスト (共通 Core) と `bench/session` の確保の数・命令数の比較

## 受け入れ条件

- [ ] `bench/session/main.cirka` の確保の数が `direct.cirka` と同じになる
- [ ] ほかのベンチが遅くならない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
