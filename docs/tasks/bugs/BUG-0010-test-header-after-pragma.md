# BUG-0010: テストのヘッダが `{-# LANGUAGE #-}` の後にあると読まれない

- 状態: done
- 重大度: major
- 発見箇所: FEAT-0125 (`tests/achc/typeck/ok/ownership_warn.hs` の `-- flags:`)
- 関連: FEAT-0123, FEAT-0124

## 現象

テストハーネス (`cirka_testkit::Header::parse`) はファイル先頭のコメント行だけをヘッダとして読み、最初のコメントでない行で
止まる。Haskell のテストは先頭に `{-# LANGUAGE ... #-}` を書くので、その後の `-- max-array-copies:` や `-- flags:` が
黙って無視されていた。`tests/achc/run/h3/{array,uarray,borrow,borrow_sugar,strict_linear_tuple}.hs` の
配列のコピー回数の上限は検査されていなかった。

## 再現手順

```sh
# -- flags: --ownership=warn を pragma の後に書いたテストが strict のまま検査される
cargo test --test achc achc_typeck
```

## 期待される動作

pragma の行を飛ばしてヘッダを読む。

## 原因

`Header::parse` が `--` / `//` / `;` / `#` で始まらない行で読むのをやめていた。

## 修正

- [x] 回帰テストを追加 (`cirka-testkit` の `header` の単体テスト)
- [x] 修正 (`{-#` で始まり `#-}` で終わる行を飛ばす)

## メモ

- 修正後、上記の h3 のテストはすべて上限内だった (コピー 0、`array`/`uarray` は意図した 1 回以内)。
- `-- ghc: skip` は run テストが先頭 5 行を独自に調べていたので影響はなかった。
