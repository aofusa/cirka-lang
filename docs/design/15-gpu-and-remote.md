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
- ホストに GPU がある場合は、デバイス (`/dev/dri`) をコンテナに渡して実機の Vulkan ドライバでも動かせる
  (任意。`CIRKA_VULKAN_ICD` で ICD を選ぶ。`scripts/env.sh` が `VK_DRIVER_FILES` / `VK_ICD_FILENAMES` にする)。
- Docker イメージへの追加パッケージ: `libvulkan-dev` (ヘッダとローダ。bindgen のテストが使うので FEAT-0048 で追加済み)。
  FEAT-0251 で `mesa-vulkan-drivers`、`vulkan-tools`、`vulkan-validationlayers`、`glslc` (GLSL → SPIR-V)、`spirv-tools` を加えた
  (Ubuntu 24.04 では lavapipe は `llvmpipe (LLVM 20.1.2)`、Vulkan 1.4)。`scripts/ci.sh` はこれらがあることを確かめる。

### 2.2 構成

| 層 | 内容 |
|----|------|
| `Vulkan.Sys` (`lib/cirkalang/vulkan/Vulkan/Sys.cirka`) | `cirka bindgen --module=Vulkan.Sys --link=vulkan` で `vulkan_core.h` から生成した extern 宣言 (FEAT-0048, 0250)。生成物をコミットし、テストで生成し直した結果と比べる |
| `Vulkan` (安全なラッパ) | `withGpu` が作るリージョン `s` に資源 (`Gpu s` / `Buffer s e` / `Pipeline s`) の型を結び付け、作った資源の破棄の処理をリージョンの中の一覧に積んで、終わりに作った逆順に実行する (ADR-097)。構成子は非公開 (ADR-096)。バッファは host-visible・coherent のメモリを作成時からマップしておく。`dispatch` は同期 |
| シェーダ | GLSL で書き、`glslc` で SPIR-V にする。実行時にファイルから読む (`spirvFile`) か、`glslc -mfmt=c` の配列を C のファイルでリンクして埋め込む (`spirv`)。CirkaLang から SPIR-V を生成することは対象外 (第二段階で検討) |

当初の案は各ハンドルを `LType` + `Release` で包み、破棄の順序を所有関係で、マップを借用で表すものだった。
バッファがデバイスより先に破棄されることを線形型で表すには、子が親の借用を持ち続ける入れ子が要り、`Release` は親のハンドルを受け取れない。
リージョンと破棄の一覧なら、順序は積んだ順で自動的に正しく、使用後の破棄と破棄漏れは v0.0.3 のリージョンの型規則 (E1200) で防げる (ADR-097)。
API と規則の詳細は [spec-v0.0.6.md](cirkalang/spec-v0.0.6.md) §3。

### 2.3 サンプルと検証

- ベクトル加算、SAXPY、行列積、リダクション (`tests/cirkalang/run/v006/vulkan_compute.cirka`)。CPU (CirkaLang) で計算した結果と完全に一致することを確認する
  (値は整数か 2 進で正確に表せる数にして、浮動小数の誤差がないようにしている)。
- 検証レイヤー (`VK_LAYER_KHRONOS_validation`) を有効にして実行し、その出力 (標準出力) がないことを期待出力との比較で確かめる。
- ワークグループ数を部分集合型で表し (`{x : Int | 1 <= x && x <= 65535}`)、v0.0.4 の検証機能が実用 API に使えるかを評価した
  (spec-v0.0.6.md §3.5: 静的に決まる制約には向き、他の値に依存する制約は実行時の検査にした)。

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
