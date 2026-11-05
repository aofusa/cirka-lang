# FEAT-0125: .chs モード・CirkaOwnership・ownership レベル

- 状態: done
- トラック: aCHC
- マイルストーン: H3 所有権拡張 (.chs)
- 優先度: P1
- 依存: FEAT-0122
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md)

## 概要

.chs 拡張子と pragma、`ownership = off|warn|strict`、診断の改善。

## 作業内容

- [x] モード切替と既定拡張 (`.chs` は `CirkaOwnership`・`LinearTypes`・`BangPatterns`・`LambdaCase`・`ScopedTypeVariables`)
- [x] .hs と .chs の相互 import (ローダは両方を探す。run テストのハーネスが `.chs` と `Main.chs` を扱う)
- [x] 診断の重大度切替 (`--ownership=off|warn|strict`・`achc.toml`。リークの診断 E0302/E0304 だけ。ADR-042)
- [x] 共通ドライバ: 知らない `--名前=値` をフロントエンドに渡す (`Options::frontend_flags`。`cirka` と `.core` 入力では拒否)

## 受け入れ条件

- [x] .hs と .chs が混在するプロジェクトの run テスト (`tests/achc/run/h3/mixed/`: `Main.hs` → `Counter.chs` → `Util.hs`、`.warn` で警告を確認)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 型検査のテスト: `tests/achc/typeck/ok/ownership_{warn,off}.hs`、`fail/ownership_{off,pragma}.hs`、`fail/ownership_chs.chs`。
- 作業中に BUG-0010 (pragma の後のテストのヘッダが読まれない) を見つけて修正した。
- `achc.toml` の `default-extensions` と `cycles` はまだ使っていない (`cycles` は FEAT-0126)。

