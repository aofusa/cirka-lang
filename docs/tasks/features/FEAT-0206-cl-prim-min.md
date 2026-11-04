# FEAT-0206: プリミティブ型と最小ライブラリ

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0205
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md), [09-libraries.md](../../design/09-libraries.md)

## 概要

Int/UInt/Float/Bool/Char、算術、比較、最小の出力 (`println`)、`Unique` と `Drop` の最小限。

## 作業内容

- [x] `lib/cirkalang/core` の最小部
- [x] プリミティブ演算の Core への対応
- [x] `Drop` の自動挿入 (最小)

## 受け入れ条件

- [x] run テスト
- [x] 所有権エラーの fail テスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- `lib/cirkalang/core/Prelude.cirka`: プリミティブ型 (`Int` `UInt` `Float` `Char` `String` `IO` `Unique`)、
  `Bool` `Unit` `Nat` `Pair` `List` `Maybe` `Either`、算術・比較・ビット演算、`Float` 演算 (`+.` など)、文字・文字列、
  入出力 (`putStr` `putStrLn` `putChar` `printInt` `printFloat` `readInt`)、`exit` `panic`、リスト関数。
  概要の `println` は `putStrLn` / `printInt` として実装した (多相な `show` はトレイトとともに v0.0.2)。
- プリミティブの Core への対応は FEAT-0204 の `cirkalang-lower` (`prim_body`) と `support.core`。
- `Drop` は spec §4.4 の組み込みの Weaken-Drop のみ。Core では v0.0.1 のすべての多重度を ω にし、
  捨てられた値は Perceus の `drop` が解放する (明示的な Drop の挿入は利用者定義の `Drop` が入る v0.0.2)。
- テスト: run は FEAT-0205 の `tests/cirkalang/run/v001` に加え `drop.cirka` (使わない線形値の暗黙の破棄・リーク 0)。
  所有権エラーは `tests/cirkalang/fail/own` (E0301–E0306)。
- 見つけて直したもの: 引数の型が `List ?a` のように未解決のメタ変数を含むと資源を持たない型と判定できず、
  線形な構造のフィールド `xs : List Int` を `length xs` に渡せなかった。引数を検査してから判定する (spec §4.2 に追記)。
