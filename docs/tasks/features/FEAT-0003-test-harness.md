# FEAT-0003: 統合テストハーネス (run / fail / mir / diff)

- 状態: todo
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

10-testing.md §2 のハーネスを作る。コンパイラ本体がない段階では `mir` 種別(手書き MIR)から使う。

## 作業内容

- [ ] `tests/harness`: テストファイルの列挙・ヘッダコメント (`achc-flags` 等) の解析
- [ ] 期待ファイル (`.stdout`/`.stderr`/`.exitcode`) との比較と `ACHC_BLESS=1` による更新
- [ ] 失敗時の成果物を `_build/test-failures/` に保存
- [ ] `CIRKA_TEST_JOBS` による並列実行
- [ ] `diff` 種別: `CIRKA_GHC` で GHC の結果を取得・キャッシュ (未設定ならスキップ)

## 受け入れ条件

- [ ] ダミーのテストケースで成功・失敗・bless がそれぞれ期待どおり動く
- [ ] ハーネス自体のユニットテストがある
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

