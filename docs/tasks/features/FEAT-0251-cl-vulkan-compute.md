# FEAT-0251: Vulkan コンピュートシェーダ (Docker / lavapipe)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P0
- 依存: FEAT-0250
- 設計: [15-gpu-and-remote.md](../../design/15-gpu-and-remote.md), [08-ffi.md](../../design/08-ffi.md)

## 概要

Docker 内 (GPU なし) で Mesa lavapipe を使い、Vulkan のコンピュートシェーダによる計算を行う。ウィンドウ・描画は扱わない。

## 作業内容

- [ ] Dockerfile に libvulkan-dev / mesa-vulkan-drivers / vulkan-tools / vulkan-validationlayers / glslc / spirv-tools を追加
- [ ] `vulkan-sys` (bindgen) と安全なラッパ `cirka-vulkan` (ハンドルを LType + Release で管理、マップは借用)
- [ ] GLSL → SPIR-V のビルド時変換と埋め込み
- [ ] サンプル: ベクトル加算・SAXPY・行列積・リダクション

## 受け入れ条件

- [ ] CPU の計算結果と一致する
- [ ] 検証レイヤーでエラー・警告がない
- [ ] 破棄漏れが型エラーになる
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

