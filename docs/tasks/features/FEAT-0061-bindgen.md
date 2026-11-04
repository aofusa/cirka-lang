# FEAT-0061: achc bindgen と import "C"

- 状態: todo
- マイルストーン: M6 FFI 拡張
- 優先度: P2
- 依存: FEAT-0060
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

libclang で C ヘッダを解析し extern ブロックを生成する。

## 作業内容

- [ ] libclang の利用 (`libclang-18-dev`)
- [ ] 関数・構造体・列挙・定数の変換
- [ ] `import "C"` の糖衣とキャッシュ

## 受け入れ条件

- [ ] `math.h` から生成した宣言で sin/cos が呼べる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

