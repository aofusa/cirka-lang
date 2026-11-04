# FEAT-0250: FFI の完成と bindgen の利用

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 GUI (Vulkan)・数値計算
- 優先度: P0
- 依存: FEAT-0243, FEAT-0047, FEAT-0048
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

構造体・共用体・コールバック・ハンドル型を CirkaLang の extern で扱えるようにする。

## 作業内容

- [ ] extern struct / union
- [ ] コールバック
- [ ] bindgen 出力の CirkaLang 版

## 受け入れ条件

- [ ] C ライブラリのサンプル (zlib など) が RAII で動く
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

