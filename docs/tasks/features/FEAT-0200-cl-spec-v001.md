# FEAT-0200: CirkaLang v0.0.1 言語仕様

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: なし
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

12-cirkalang.md をもとに v0.0.1 の構文・型規則 (QTT・Type/LType)・評価規則・診断を確定する。

## 作業内容

- [x] 構文 (EBNF)
- [x] 型付け規則 (QTT + 宇宙 Type/LType)
- [x] 消去の規則
- [x] v0.0.1 の対象外機能の明記

## 受け入れ条件

- [x] `docs/design/cirkalang/spec-v0.0.1.md` がレビュー済み
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ


- 仕様書: [spec-v0.0.1.md](../../design/cirkalang/spec-v0.0.1.md)。主な決定: Type-in-Type (v0.0.4 で階層化)、型・暗黙引数は既定で多重度 0、`let` の多重度は型から推論、v0.0.1 の型は (コンストラクタのない/Drop のないフィールドを持つ LType のデータ型を除き) 組み込みの Drop を持つ、網羅性は必須、数値は Core で箱入り (`I# Int#` など。箱の除去は Core 最適化と v0.0.3 の性能作業で)、`IO a` は Core で `Unit# -> a`。
- 自己レビュー: 12-cirkalang.md・03-type-system.md (U-Bind・Weaken-Drop・診断コード E030x) と整合することを確認。
- テスト: `tests/workspace.rs::cirkalang_spec_for_current_version` (開発中の版の仕様書が存在し、必要な節を持つ)。
