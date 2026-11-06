# FEAT-0216: ソース内のテスト (`test` 宣言と `cirka test`)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P1
- 依存: FEAT-0207
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md) §11、ADR-048

## 概要

Rust の `#[test]` / `#[cfg(test)]` のように、プログラムの本体とテストを一つのソースファイルに同居させる。
FEAT-0132 のレビューでの要望。

## 作業内容

- [ ] 構文: `test "名前" = 式` (文脈キーワード) と `tests where { 宣言 }` ブロック (cirkalang-syntax)
- [ ] `build` / `run` では構文解析の直後にテストを捨てる。`check` と `test` では残す
- [ ] 名前の重複 (E1112)、テスト専用の名前をテスト以外から使う誤り (E1113)
- [ ] テスト用の `main` を生成する (引数のテスト番号でテストを選ぶ。`main` は不要)
- [ ] `cirka test [path] [--filter s]`: 実行ファイルを作り、テストごとに別プロセスでリークチェック付きで走らせ、結果と集計を出す
- [ ] Prelude に `assert`・`assertEqInt` など
- [ ] spec と 12-cirkalang.md §11 を実装に合わせる

## 受け入れ条件

- [ ] 成功・失敗 (assert・panic・exit・リーク) の各場合を確かめる統合テスト (`tests/cirkalang/test/`)
- [ ] `cirka build` の生成物にテストが残らないことを確かめるテスト
- [ ] E1112 / E1113 の fail テスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
