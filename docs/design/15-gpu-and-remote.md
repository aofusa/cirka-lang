# 15. GPU (Vulkan compute) と遠隔環境での GUI 検証

## 1. 方針

| 環境 | 対象 | 位置づけ |
|------|------|----------|
| **Docker (x86_64 Linux)** | **Vulkan のコンピュートシェーダによる計算のみ** (ウィンドウなし) | CirkaLang v0.0.6 の必須範囲 |
| 遠隔の macOS / Windows (ssh) | ウィンドウを開いて描画する GUI | **任意** (可能であれば行う)。v0.0.6 のリリース条件には含めない |

- ターゲットは引き続き x86_64 Linux。Docker 内には GPU も表示装置もないため、GUI は扱わない (ADR-028)。
- 遠隔ホストの接続先 (ssh のホスト名・別名) と作業ディレクトリは **環境依存の情報なので `.env` だけに書き、git で管理しない** (ADR-029)。
  設計書・チケット・スクリプト・コミットメッセージには変数名だけを書き、実際のホスト名やパスを書かない。

## 2. Vulkan compute (Docker 内、CirkaLang v0.0.6)

### 2.1 実行環境

- Mesa の **lavapipe** (CPU で動くソフトウェア Vulkan 実装) を使い、GPU なしで Docker 内でテストできるようにする。
- ホストに GPU がある場合は、デバイス (`/dev/dri`) をコンテナに渡して実機の Vulkan ドライバでも動かせる (任意。`CIRKA_VULKAN_ICD` で ICD を選ぶ)。
- Docker イメージへの追加パッケージ (FEAT-0251 で追加する): `libvulkan-dev`、`mesa-vulkan-drivers`、`vulkan-tools`、
  `vulkan-validationlayers`、`glslc` (GLSL → SPIR-V)、`spirv-tools`。

### 2.2 構成

| 層 | 内容 |
|----|------|
| `vulkan-sys` | `cirka bindgen` で `vulkan_core.h` から生成した extern 宣言 (FEAT-0048, 0250) |
| `cirka-vulkan` (安全なラッパ) | `Instance` / `Device` / `Buffer` / `DeviceMemory` / `ShaderModule` / `Pipeline` / `CommandPool` を `LType` + `Release` で RAII 管理。破棄の順序 (依存するハンドルから先に) は所有関係で表す。メモリのマップは借用 (`MutRef r`) で表し、アンマップ忘れを防ぐ |
| シェーダ | 当面は GLSL で書き、ビルド時に `glslc` で SPIR-V にしてバイト列として埋め込む。CirkaLang から SPIR-V を生成することは対象外 (第二段階で検討) |

### 2.3 サンプルと検証

- ベクトル加算、SAXPY、行列積、リダクション。CPU (CirkaLang) で計算した結果と一致することを確認する。
- 検証レイヤー (`VK_LAYER_KHRONOS_validation`) を有効にして、エラー・警告がないことをテストで確認する。
- バッファサイズやワークグループ数を部分集合型で表し (例: `{n : Nat | n <= maxWorkGroupCount}`)、v0.0.4 の検証機能が実用 API に使えるかも評価する。

## 3. 遠隔環境での GUI 検証 (任意)

### 3.1 `.env` の変数

| 変数 | 内容 |
|------|------|
| `CIRKA_REMOTE_MAC_HOST` | macOS の ssh 接続先 (`ssh <値>` で接続できる名前) |
| `CIRKA_REMOTE_MAC_WORKDIR` | macOS 上の作業ディレクトリ (指定された親ディレクトリの中に本プロジェクト用のディレクトリを作る) |
| `CIRKA_REMOTE_WIN_HOST` | Windows の ssh 接続先 |
| `CIRKA_REMOTE_WIN_WORKDIR` | Windows 上の作業ディレクトリ (同上) |

値が空なら、その遠隔検証は行わない (スキップ)。実際の値は利用者の `.env` にのみ書く。

### 3.2 前提: ターゲットの移植

aCHC / CirkaLang は x86_64 Linux 向けのコードしか生成しないため、GUI を遠隔で検証するには対象 OS への移植が必要になる。

| 対象 | 必要な作業 | チケット |
|------|------------|----------|
| macOS | Mach-O 出力・ターゲットトリプル (ホストの CPU に合わせる)、ランタイムの POSIX 移植 (`mmap`・pthread はそのまま、epoll の代わりに kqueue)、Vulkan は MoltenVK (Vulkan SDK) | FEAT-0053 |
| Windows | PE/COFF 出力 (x86_64-pc-windows-msvc を第一候補)、ランタイムの Win32 移植 (`VirtualAlloc`・スレッド・コンソール I/O・スタックサイズ)、アンワインドは SEH (当面 abort) | FEAT-0054 |

- **ビルドは遠隔ホスト上でネイティブに行う** (ソースを同期し、遠隔ホストの Rust と clang/LLVM でコンパイラとランタイムをビルドする)。
  macOS 向けの Docker からのクロスビルドは SDK とリンカの問題で現実的でないため。
- 遠隔ホストに必要なもの: Rust、clang/LLVM、Vulkan SDK (macOS は MoltenVK を含む)、GLFW (ウィンドウ作成)。

### 3.3 手順 (`scripts/remote.sh`、FEAT-0052)

```
scripts/remote.sh <mac|win> check   # 接続と前提ツールの確認
scripts/remote.sh <mac|win> sync    # git 管理下のファイルだけを作業ディレクトリへ同期 (.env・target は送らない)
scripts/remote.sh <mac|win> build   # 遠隔でビルド
scripts/remote.sh <mac|win> run <sample>   # 遠隔でサンプルを実行
scripts/remote.sh <mac|win> fetch   # ログ・画像を docs/artifacts/remote/<mac|win>/ に回収
```

- スクリプトは `.env` から接続先と作業ディレクトリを読む。ホスト側 (`~/.ssh/config` の別名と鍵) で実行する (コンテナ内からではない)。
- 同期は macOS では `rsync`、Windows では OpenSSH の `scp` + `tar` を使う (Windows には rsync がない場合が多い)。
- 作業ディレクトリの外は変更しない。遠隔での削除は作業ディレクトリ内に限る。

### 3.4 GUI の検証方法

| 方法 | 内容 | 注意 |
|------|------|------|
| オフスクリーン描画 | 画像に描画して回収し、参照画像と比較 | ssh 経由でも確実。表示装置がなくても検証できる |
| ウィンドウの起動確認 | ウィンドウを開いて N フレーム描画して終了し、検証レイヤーのログを確認 | macOS: ログイン中のユーザーのセッションが必要。画面収録の権限がないとスクリーンショットは取れない。Windows: ssh から起動したプロセスは対話セッションに表示されないため、タスクスケジューラ (`schtasks` の対話実行) などで対話セッションに起動する |

結果 (ログ・画像) は `docs/artifacts/remote/` に置き (git 管理外)、結論だけをチケットに記録する。

## 4. チケット

| チケット | 内容 |
|----------|------|
| FEAT-0251 | Vulkan compute サンプル (Docker / lavapipe) — v0.0.6 の必須範囲 |
| FEAT-0052 | 遠隔検証スクリプト (`scripts/remote.sh`) |
| FEAT-0053 / FEAT-0054 | macOS / Windows ターゲットへの移植 |
| FEAT-0290 | 遠隔環境での GUI サンプル (ウィンドウ + 三角形描画) |
