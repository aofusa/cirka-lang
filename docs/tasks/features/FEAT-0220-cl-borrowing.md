# FEAT-0220: 借用

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P0
- 依存: FEAT-0215
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

スコープ付き借用 (`Ref r a` / `MutRef r a`) を基本に、ライフタイムを省略できる借用構文 (`&x`, `&mut x`) を試す。

## 作業内容

- [x] スコープ付き借用 (`Data.Borrow`: `borrow` / `borrowIO`、`Ref a r`、`Borrow a r` / `BorrowIO a r` のモナド)
- [x] 借用構文の糖衣と省略規則の試作 (`borrow x as a in e`。リージョンを書かない)
- [x] 借用越しの操作で RC 操作が出ないことの確認 (状態トークンの worker の一般化、CPR が `cast` 越しの末尾呼び出しを見る)

## 受け入れ条件

- [x] 借用の持ち出しが型エラーになるテスト (`fail/type/borrow_escape`)
- [x] 借用版と所有版の性能比較 (`run/v003/borrow_loop` と `owned_loop`、`cirkalang_borrow_costs_nothing`)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: 借用は所有者を線形に受け渡す状態のモナドで、リージョン `r` は `borrow` が渡す多重度 0 の引数 (spec-v0.0.3.md §2、ADR-071)。
  `Ref` / `MutRef` は 1 種類にまとめた。`&x` / `&mut x` は採らず、糖衣 `borrow x as a in e` だけを足した。
- `impl` の変数の種の宣言 `impl {a : LType} ->` を足した (線形な型への `Monad (Borrow a r)` に必要。ADR-072)。
- 持ち出しは E1200 で、「リージョンは借用の中にしかない」注記を付け、連鎖する E1210 を出さない。
- 性能 (挿入ソート 20000 要素、ネイティブ、`CIRKA_STATS=1`):

  | 版 | 時間 | 確保 |
  |----|------|------|
  | 所有者を手で受け渡す | 0.08s | 9 |
  | 借用 (最適化前) | 1.53s | 100,129,789 |
  | 借用 (状態の worker の一般化後) | 0.08s | 40,009 (結果が `()` の `sink` の組と `Ur`。FEAT-0081) |

  結果が `Int` の版 (`run/v003/borrow_loop`) はインタプリタで確保・RC 操作数が所有版と同じ (入口の定数 1 を除く)。
