# FEAT-0251: Vulkan サンプル (GUI)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 GUI (Vulkan)・数値計算
- 優先度: P1
- 依存: FEAT-0250
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

Vulkan と窓システム (GLFW など) のバインディングで三角形を描画する。Vk ハンドルを RAII で管理。

## 作業内容

- [ ] Dockerfile に Vulkan/GLFW の開発パッケージを追加
- [ ] バインディング生成
- [ ] 三角形描画サンプル (ヘッドレスでのテスト方法も用意)

## 受け入れ条件

- [ ] サンプルが動作し、検証レイヤーでエラーがない
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

