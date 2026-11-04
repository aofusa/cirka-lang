# FEAT-0290: 遠隔環境での GUI サンプル (ウィンドウ + 描画)

- 状態: todo
- トラック: CirkaLang
- マイルストーン: CX 遠隔環境での GUI 検証 (任意)
- 優先度: P2
- 依存: FEAT-0052, FEAT-0053, FEAT-0054, FEAT-0253
- 設計: [15-gpu-and-remote.md](../../design/15-gpu-and-remote.md)

## 概要

遠隔の macOS (MoltenVK) / Windows で、ウィンドウを開いて Vulkan で三角形を描画する。可能な範囲で行う任意のチケット。

## 作業内容

- [ ] GLFW のバインディング
- [ ] スワップチェーンと描画パイプライン
- [ ] オフスクリーン描画と参照画像の比較
- [ ] ウィンドウ起動確認 (macOS はログインセッション、Windows は対話セッションへの起動)
- [ ] 結果を docs/artifacts/remote/ に回収

## 受け入れ条件

- [ ] 少なくとも一方のホストで描画を確認し、結果をチケットに記録
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

