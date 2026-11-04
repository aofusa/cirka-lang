# FEAT-0202: CirkaLang 名前解決とモジュール

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0201
- 設計: [12-cirkalang.md](../../design/12-cirkalang.md)

## 概要

モジュール (ファイル単位)・import・名前解決。

## 作業内容

- [x] モジュール探索
- [x] import / 公開範囲
- [x] 名前解決の診断

## 受け入れ条件

- [x] 複数モジュールのテスト
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ


- 実装: `cirkalang-resolve`。`Loader` (ファイル: ソースルートから `A/B.cirka`、メモリ: テスト用) でモジュールを読み、依存順に並べる (循環は E1105)。名前は自モジュールの定義が import より優先、import 同士の衝突は使用時に E1102、修飾名 `A.B.x`、`private` (E1106)。
- 演算子列を fixity (全モジュールの宣言を集めたもの。既定 infixl 9) で Haskell 2010 §10.6 の方法で結合し、前置 `-` は `negate`。セクションと `let f x = e` を脱糖。シグネチャの未束縛の小文字名を暗黙引数 `{0 x : _}` にする。
- テスト: 単体テスト (fixity・自動暗黙引数・各種エラー・曖昧さと修飾名・依存順)、`tests/cirkalang/modules/` (入れ子のモジュールパスを含む複数モジュールのスナップショット `.rn` とエラー `.stderr`)。
