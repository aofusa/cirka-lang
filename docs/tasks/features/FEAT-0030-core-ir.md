# FEAT-0030: 共通 Core IR と Core Lint

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M3 共通 Core と Core→MIR
- 優先度: P0
- 依存: FEAT-0005
- 設計: [06-ir.md](../../design/06-ir.md)

## 概要

両フロントエンドの合流点となる「消去後の Core」。依存型を消去した結果を表すための `Any` 型と `cast` を含む。

## 作業内容

- [ ] Core のデータ構造・プリティプリンタ
- [ ] 多重度 (0/1/ω)・`let`/`let!`・`Any`/`cast`
- [ ] Core Lint (型・種・多重度・U-Bind・スコープ)
- [ ] テスト用の Core 構築 DSL

## 受け入れ条件

- [ ] 誤った Core を Lint が検出するテスト
- [ ] Haskell 由来・CirkaLang 由来どちらの形も表せることを例で確認
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

