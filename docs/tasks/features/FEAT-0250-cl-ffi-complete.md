# FEAT-0250: FFI の完成と bindgen の利用

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.6 Vulkan compute・数値計算
- 優先度: P0
- 依存: FEAT-0243, FEAT-0047, FEAT-0048
- 設計: [08-ffi.md](../../design/08-ffi.md)

## 概要

構造体・共用体・コールバック・ハンドル型を CirkaLang の extern で扱えるようにする。

## 作業内容

- [x] extern の構造体・共用体 (`struct` / `union` / `packed struct`): 構成子、フィールドの関数 `S.f` / `S.set_f` / `S.ptr_f`、共用体のメンバーのポインタ、
      `Storable`、値渡し (x86_64 SysV の分類は共通基盤 FEAT-0047)
- [x] 列挙・定数・型の別名・不透明型 (ハンドル型は不透明型へのポインタで、`Release` を持つ線形な型に包む)、C の型のリテラル (`42 : CInt`)
- [x] 可変長引数 (`...`)、記号名の指定 (`"deflateInit_" deflateInit`)
- [x] コールバック (`"wrapper"`、線形な `Callback`、`freeFunPtr`)、関数ポインタの呼び出し (`"dynamic"`)、記号のアドレス (`"&sym"`)
- [x] 任意のライブラリの `link` (Core / MIR の `libs`、ドライバの `-l`、インタプリタの読み込み)
- [x] bindgen 出力の CirkaLang 版 (関数ポインタの型の `dyn_T` / `wrap_T`、`shapes`・`vulkan` の実行テスト)
- [x] `Foreign` に `CBool`・`FunPtr`・`CArray`・`Storable` と補助関数、impl の値の変数を実行時に渡す
- [x] 仕様 (spec-v0.0.6.md §1)、08-ffi.md §7.3、ADR-094

## 受け入れ条件

- [x] C ライブラリのサンプル (zlib など) が RAII で動く — `tests/cirkalang/run/v006/zlib.cirka` (線形な `Stream` を `deflateEnd` / `inflateEnd` で解放、エラーの経路も含めてリークなし、ネイティブとインタプリタ)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- テスト: 実行 `v006/extern_records` (構造体・共用体の値渡しを SysV の各分類で、入れ子・配列・packed、等価性、列挙、定数、`"dynamic"`・`"&sym"`、記号名、可変長)、
  `v006/extern_callbacks` (レジスタ・メモリの構造体を受け渡すコールバック、クロージャ、`qsort`、`Callback` の解放。ネイティブのみ)、`v006/zlib`。
  診断 `type/extern_decls` (16 の誤り)、`syntax/extern_variadic`、`syntax/extern_abi` (ライブラリの名前)。
  bindgen `gen/features` (生成した宣言の検査)、`run/shapes` と `run/vulkan` (Docker。Haskell 版と同じ値: 構造体の大きさ 48・64・824・504、定数、ローダの版)。
- 共通基盤の追加: Core の `Program` と MIR の `Module` の `libs`、ドライバの `native::lib_args`、インタプリタの `lib名前.so` の読み込み (08-ffi.md §7.3)。
- Docker イメージに `zlib1g-dev` を加えた (zlib のサンプルのリンクに `libz.so` が要る)。
- `vulkan_core.h` 全体の出力 (約 2 万行) は `cirka check` を通るが約 50 秒かかる。原因はインスタンス探索で impl の一覧を毎回複製して線形に試すこと → FEAT-0255。
- 構造体・共用体の値渡しは呼び出しごとに `malloc` / `free` する。Vulkan の API の大半はポインタ渡しなので影響は小さい。スタックに置く最適化は、効くベンチを計測してから行う (16-performance.md §4)。
- 作業中に見つけたバグ: BUG-0026 (型の中の多重定義された演算でインスタンスの解決が ICE)。
