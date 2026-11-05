# FEAT-0124: 借用 (Ref / MutRef / Borrow モナド)

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0121
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

02 §5 の借用 API と構文糖。

## 作業内容

- [x] `Ref`/`MutRef`/`Borrow r`/`borrowIn` (`Cirka.Borrow`、実装は `Unsafe.Borrow`。ほかに `borrowMutIn`・`asRef`)
- [x] 借用操作の消去 (newtype と `IO` の newtype。所有者の dup/drop 1 組だけ)
- [x] 構文糖 (`CirkaBorrow`: `borrow e as x in body`)
- [x] E0320 (ランク N 引数のスコーレムの脱出検査を追加。借用以外の脱出は E0604)
- [x] 配列の借用操作 `lengthRef`・`index`・`writeAt` (`Cirka.Array`、`Cirka.Array.Unboxed`) と共有書き込みのプリミティブ

## 受け入れ条件

- [x] クロージャ経由を含め借用の持ち出しが型エラーになる (`tests/achc/typeck/fail/borrow_escape.hs`)
- [x] テストを追加し `scripts/ci.sh` が通る (`tests/achc/run/h3/{borrow,borrow_sugar}.hs`)

## メモ

- 修正前はランク N の引数の剛体変数がメタ変数を通して外へ出るのを検査していなかった
  (`borrow arr (\a -> return a)` が型検査を通っていた)。
- 結果の共有可能な値は `Ur` で包んで取り出す (線形の `case` の束縛になるため)。
- 制限: 入れ子の借用の内側では外側の借用を使えない (リージョンの部分型がない)。線形の値を `Borrow r` の `do` で
  受け渡せない。`MutRef` の書き込みは所有者の一意性を型 (U-Bind) だけで保証する (実行時の確認はしない。ADR-041)。

