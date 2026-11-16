# FEAT-0251: Vulkan コンピュートシェーダ (Docker / lavapipe)

- 状態: in-progress
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P0
- 依存: FEAT-0250, FEAT-0256
- 設計: [15-gpu-and-remote.md](../../design/15-gpu-and-remote.md), [08-ffi.md](../../design/08-ffi.md)

## 概要

Docker 内 (GPU なし) で Mesa lavapipe を使い、Vulkan のコンピュートシェーダによる計算を行う。ウィンドウ・描画は扱わない。

## 作業内容

- [x] Dockerfile に mesa-vulkan-drivers / vulkan-tools / vulkan-validationlayers / glslc / spirv-tools を追加 (libvulkan-dev は FEAT-0048 で追加済み)
- [x] `Vulkan.Sys` (vulkan_core.h の bindgen の出力、`lib/cirkalang/vulkan/Vulkan/Sys.cirka`) と安全なラッパ `Vulkan`
      (当初の「ハンドルを LType + Release で管理、マップは借用」をやめ、リージョンと破棄の処理の一覧で管理する。ADR-097)
- [x] GLSL → SPIR-V の変換と埋め込み (`glslc -mfmt=c` の配列を C でリンクする `spirv`、ファイルから読む `spirvFile`。テストハーネスが `.comp` を変換する)
- [x] サンプル: ベクトル加算・SAXPY・行列積・リダクション (`tests/cirkalang/run/v006/vulkan_compute.cirka`)
- [x] 部分集合型でワークグループ数を表し、検証機能が実用 API に使えるかを評価する (spec-v0.0.6.md §3.5)
- [x] `scripts/ci.sh` に「Vulkan compute」の段 (glslc・Vulkan のデバイス・検証レイヤーがあること)

## 受け入れ条件

- [x] CPU の計算結果と一致する (4 つの計算とも全要素が一致)
- [x] 検証レイヤーでエラー・警告がない (検証レイヤーを有効にして実行し、その出力がないことを標準出力の比較で確かめる)
- [x] 破棄漏れが型エラーになる (資源を `withGpu` の外に出すと E1200。作った資源はすべて破棄の一覧に積まれ、構成子は非公開なので、
      破棄されない資源を作る方法がない。破棄をわざと抜くと検証レイヤーが報告してテストが失敗することも確かめた)
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ

### 結果

- `Vulkan` のモジュール (約 650 行) は `Data.Region` の `withRegion` / `RIO` / `RefIn` と非公開の構成子 (FEAT-0256) だけで書け、新しい言語機能は要らなかった。
  `withGpu` は計算が `Err` で止まっても、作った資源を作った逆順に破棄する (`dispatch` の引数の数の誤り・`write` の要素の多すぎで確かめた)。
- 実行テスト `vulkan_compute` は lavapipe 上で、インタプリタ (-O0 / -O2) とネイティブ (debug / release のランタイム) のすべてで
  CPU の結果と一致し、リークもない。インタプリタは libvulkan を `dlopen` して同じ API を呼ぶ。
- fail テスト: `fail/vulkan/escape` (E1200)、`fail/vulkan/private` (E1106: `Vk.MkBuffer`)、`fail/vulkan/groups` (E0501: 0 と 65536)。
- `-O0` と `cirka check` で、引数 8 個以上の `IO` の extern 関数 (`vkCmdBindDescriptorSets`・`vkCmdPipelineBarrier`) が C0200 になる
  共通基盤のバグを見つけ、BUG-0027 で直した (部分適用のカリー化)。
- `Std.Fs.fileStatus` はインタプリタで動かないので、`spirvFile` は C の stdio (`fopen` / `fseek` / `ftell` / `fread`) で読む。

### 部分集合型の評価

- ワークグループ数 `{x : Int | 1 <= x && x <= 65535}` は、リテラルなら評価だけで決まり (Z3 不要)、0 や 65536 は E0501 になる。
  計算した数も `if` の条件から示せる。消去されるので実行時のコストはない。静的に決まる仕様の制約には十分に実用的である。
- バッファの要素数・`dispatch` に渡すバッファやプッシュ定数の数 (パイプラインに依存) を型で表すには、パイプラインの型にその数を持たせる必要があり
  (`Pipeline s nb np` と `Vect nb (Arg s)`)、利用者のコードが重くなるので、実行時に確かめて `Invalid` を返すことにした。
  第二段階で、依存型の引数を推論で省ける範囲が広がれば型に移せる。
- デバイスごとの上限 (`maxComputeWorkGroupCount`・`maxStorageBufferRange`) は実行時の値なので、部分集合型では仕様の下限しか表せない。
