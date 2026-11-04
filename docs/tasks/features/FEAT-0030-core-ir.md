# FEAT-0030: 共通 Core IR と Core Lint

- 状態: done
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P0
- 依存: FEAT-0005
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

両フロントエンドの合流点となる「消去後の Core」。依存型を消去した結果を表すための `Any` 型と `cast` を含む。

## 作業内容

- [x] Core のデータ構造・プリティプリンタ
- [x] 多重度 (0/1/ω)・`let`/`let!`・`Any`/`cast`
- [x] Core Lint (型・種・多重度・U-Bind・スコープ)
- [x] テスト用の Core 構築 DSL

## 受け入れ条件

- [x] 誤った Core を Lint が検出するテスト
- [x] Haskell 由来・CirkaLang 由来どちらの形も表せることを例で確認
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 実装: `cirka-core` の `ty` (種・多重度・型。型変数の適用 `Type::App` を含む)、`expr` (式・プログラム)、`sexp`/`parse`/`print` (S 式のテキスト形式 `.core`。06-ir.md §1.1)、`lint`、`build` (DSL)。
- Lint の規則は 06-ir.md §1.2 に記載。`let!`/`case` の束縛は、資源を持ちえない型なら ω でも 1 回の使用と数える (Linear Haskell の case_π を実用的に緩めたもの)。
- テスト: `tests/core/run/` に aCHC 由来 (辞書・高階種・遅延無限リスト) と CirkaLang 由来 (Any/cast による消去・線形資源・let!) の例、`tests/core/fail/` に 14 個の誤った Core と期待診断。印字 → 再読込 → 印字の往復を確認。
