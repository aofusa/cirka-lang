# FEAT-0003: 統合テストハーネス

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M0 基盤整備
- 優先度: P0
- 依存: FEAT-0001
- 設計: [10-testing.md](../../design/10-testing.md)

## 概要

10-testing.md §2 のハーネス。種別 (mir / achc run・fail・diff / cirkalang run・fail・verify) を共通の仕組みで扱う。

## 作業内容

- [ ] テストファイルの列挙とヘッダコメント (`flags` 等) の解析
- [ ] 期待ファイルとの比較と `ACHC_BLESS=1` / `CIRKA_BLESS=1` による更新
- [ ] 失敗時の成果物を `_build/test-failures/` に保存
- [ ] `CIRKA_TEST_JOBS` による並列実行
- [ ] diff 種別: `CIRKA_GHC` で GHC の結果を取得・キャッシュ

## 受け入れ条件

- [ ] ダミーのテストケースで成功・失敗・bless が期待どおり動く
- [ ] ハーネス自体のユニットテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

