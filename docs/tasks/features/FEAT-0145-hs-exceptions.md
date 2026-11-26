# FEAT-0145: Control.Exception と例外の経路の Release (aCHC)

- 状態: todo
- トラック: aCHC
- マイルストーン: H5 高度な型システム (CirkaLang v0.0.4 の後)
- 優先度: P1
- 依存: FEAT-0040, FEAT-0140
- 設計: [02-language-extensions.md](../../design/02-language-extensions.md), [03-type-system.md](../../design/03-type-system.md) §8, [05-runtime.md](../../design/05-runtime.md) §6

## 概要

FEAT-0040 の `raise`/`catch` の上に、GHC 互換の `Control.Exception` (`catch`・`handle`・`try`・`throw`・`throwIO`・`evaluate`・`bracket`・
`finally`・`onException`) と `System.IO.Error` を作る。`error`・パターンの失敗・`ioError`・`exitWith` を捕まえられる例外にし、
捕まらない例外の表示 (GHC の書式) をライブラリの最上位の処理で行う。`.chs` の `Release`/`Drop` を例外の経路でも実行し、
W0305 (例外の経路で漏れうる線形の値) を実装する。`SomeException` は存在型 (FEAT-0140) で GHC と同じ形にする。

## 作業内容

- [ ] Core のプリミティブ: 例外の値がランタイムのメッセージかの区別とメッセージの取り出し
- [ ] `Control.Exception`・`System.IO.Error`・`System.Exit` (例外としての `ExitCode`)、`ErrorCall`・`IOException`・`ArithException`・
      `ArrayException`・`PatternMatchFail`
- [ ] `error`・`undefined`・パターンの失敗・`ioError` (実行したときに投げる)・`fail`・`exitWith` を例外にする。既存のテストの標準エラー出力と終了コードを保つ
- [ ] 最上位の処理 (GHC の表示の書式と、フラッシュ・後始末の順序)
- [ ] `releaseAfter`・`dropAfter` を例外に対して安全にし、`do` の線形の値の範囲を束縛から後のすべての文に広げる
- [ ] W0305
- [ ] IO の正確な例外 (GHC の IO hack に相当: 効果の後の要求を前へ動かさない)。必要なら Core の正格性解析を直す

## 受け入れ条件

- [ ] `catch`・`bracket`・`finally`・`try`・`evaluate`・`throwIO` の実行テストと GHC との差分テスト
- [ ] `.chs` の `Release` が例外の経路で実行されることを出力で確認、例外の経路でもリーク 0
- [ ] W0305 の診断テスト
- [ ] 互換の計測を更新 (`Control.Exception` を使うプログラム)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

- 互換の集まりでは 10 ファイルが `Control.Exception` を使う (`jules_xref2` は `IOException` と `catch` だけが足りない)。
