# FEAT-0214: パッケージと依存 (cirka.toml)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.2 実用 CUI
- 優先度: P1
- 依存: FEAT-0205
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

`cirka.toml` の path 依存とパッケージ単位のビルド。

## 作業内容

- [x] 依存解決 (path)
- [x] パッケージ単位の名前空間

## 受け入れ条件

- [x] 複数パッケージのプロジェクトのテスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

言語仕様は [spec-v0.0.2.md](../../design/cirkalang/spec-v0.0.2.md) §4、設計判断は ADR-058。

- `cirka.toml` の `[dependencies]` に `name = { path = ".." }` を書く。依存は推移的にたどり、菱形の依存は一つにまとめる。循環・名前の不一致などは E1118。
- 各パッケージの名前空間はパッケージ名のパスカルケース。依存のモジュールは名前空間の下にだけ置け、`import` できるのは自分と直接の依存のモジュールだけ (E1119)。名前空間の外に置いたモジュールには、見つからない理由を help で示す。
- `src/Main.cirka` のないパッケージはライブラリとして `check` / `test` できる。`cirka check` は `main` のないファイルも受け付けるようにした。
- テスト: `tests/cirkalang/packages/` (菱形の依存の実行、推移的な依存の直接 import、循環、名前の不一致、名前空間の外のモジュール、ライブラリの check / test / build)。
