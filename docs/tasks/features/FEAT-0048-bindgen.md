# FEAT-0048: bindgen (libclang による C ヘッダ取り込み)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0047
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

C ヘッダから aCHC (`.chs`) と CirkaLang (`.cirka`) の extern 宣言を生成する共通ツール。

## 作業内容

- [ ] libclang の利用
- [ ] 関数・構造体・共用体・列挙・定数・マクロ定数の変換
- [ ] 出力先言語ごとのプリンタ

## 受け入れ条件

- [ ] `math.h` と `vulkan_core.h` から宣言を生成できる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

