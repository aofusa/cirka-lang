# FEAT-0048: bindgen (libclang による C ヘッダ取り込み)

- 状態: done
- トラック: 共通基盤
- マイルストーン: SX 共通基盤の拡張 (各トラックの必要に応じて)
- 優先度: P1
- 依存: FEAT-0047
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

C ヘッダから aCHC (`.chs`) と CirkaLang (`.cirka`) の extern 宣言を生成する共通ツール。

## 作業内容

- [x] libclang の利用
- [x] 関数・構造体・共用体・列挙・定数・マクロ定数の変換
- [x] 出力先言語ごとのプリンタ

## 受け入れ条件

- [x] `math.h` と `vulkan_core.h` から宣言を生成できる
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

### 実装 (2026-11-15)

- `cirka-bindgen` が libclang を実行時に `dlopen` で読み込み (`CIRKA_LIBCLANG`、空なら `libclang-18` などを探す)、言語に依らない宣言の表を作る。
  `achc bindgen` は Haskell 2010 の FFI と `Storable`、`cirka bindgen` は extern ブロックで出力する (08-ffi.md §6、ADR-093)。
- 構造体・共用体のレイアウトは clang の値と `cirka_core::ctype` の計算を比べ、一致しないものは理由付きの不透明型にする。
  ビットフィールドは格納単位ごとの `bitfieldN`、無名の型は `<親>_<フィールド>` / `<親>_anonN`。
- マクロ定数は 2 つ目の翻訳単位の `__auto_type v = MACRO;` を clang に評価させる (関数形式のマクロを使う `VK_MAKE_API_VERSION(...)` も値になる)。
- 選ぶ宣言の既定は「ヘッダが取り込むすべて」。`--file` のパターンは宣言のファイルの絶対パス (`.`・`..` を除く) と照合する
  (相対パスで渡したヘッダに `*/x.h` が合わなかったので正規化した)。ヘッダの誤りは clang と同じ形式 (`file:line:col: error: …`) で報告し、何も出力しない。
- Docker イメージに `libvulkan-dev` を追加し、`scripts/ci.sh` は `vulkan_core.h` があることを確かめる (テストはなければスキップ)。

### テスト

- `tests/bindgen/gen/`: すべての種類の宣言と飛ばす場合を含むヘッダ (`features.h`) の aCHC / CirkaLang の出力 (生成した `.hs` は aCHC のフロントエンドも通す)、誤りのあるヘッダの診断。
- `tests/bindgen/run/`: 生成したモジュールを使うプログラムを両 CLI の -O0/-O2 で実行 (リーク 0)、Docker では GHC でも実行して一致。
  - `shapes`: C のライブラリ (構造体の読み書き・構造体の配列・ビットフィールド・共用体・`wrapper` のコールバック・`dynamic` の呼び出し・マクロ定数・列挙)。
  - `math`: libc の `math.h` (純粋と宣言した関数・出力引数・マクロと無名の列挙の定数)。`cmath`: 同じものを CirkaLang から (関数のみ。下記)。
  - `vulkan`: `vulkan_core.h` から許可リストで生成し、ローダの `vkEnumerateInstanceVersion` を呼ぶ (ドライバ不要)。構造体の大きさ
    (`VkApplicationInfo` 48・`VkInstanceCreateInfo` 64・`VkPhysicalDeviceProperties` 824・`VkPhysicalDeviceLimits` 504) は C の `sizeof` と一致。
- `math.h` と `vulkan_core.h` の全体を両言語に変換 (不透明型への後退なし)。

### 計測

| ヘッダ | 宣言 | 関数 / 構造体・共用体 | 出力 (aCHC / CirkaLang) | 時間 |
|--------|------|-----------------------|-------------------------|------|
| `math.h` (glibc 2.39) | 239 | 140 / 0 | 13.5 KB / 9.1 KB | 0.15–0.22 秒 (host) |
| `vulkan/vulkan_core.h` (1.3.275) | 4471 | 623 / 991 | 2.2 MB / 0.77 MB | 1.0 秒 (Docker) |

### 後続

- CirkaLang の出力の `type`・`enum`・`const`・`struct`・`union`・可変長引数は FEAT-0250 で使えるようになる (今は関数だけが通る。`cmath` のテスト)。
  FEAT-0250 で `tests/bindgen/run/shapes`・`vulkan` に `Main.cirka` を加える。負の数の定数 (`const NEGATIVE : CInt = -5`) もそこで受け付ける。
- aCHC の extern ブロックでの出力は FEAT-0130、`import "C"` の糖衣は extern ブロックの実装の後。

