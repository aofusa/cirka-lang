# 05. ランタイム (`cirka-runtime`)

Rust で書いた `#![no_std]` の静的ライブラリ `libcirka_rt.a`。依存は libc のみ。
生成コードからは C ABI (`extern "C"`) で呼ばれる。ホットパス(dup/drop の高速経路、確保の高速経路)は
コード生成側で LLVM IR としてインライン展開し、遅い経路だけランタイムを呼ぶ。

ビルドは `release`(既定)と `debug`(検証付き)の2種類。`achc --runtime=debug` で切り替える。

## 1. オブジェクト表現

すべての boxed オブジェクトは 8 バイトのヘッダを持つ。

```
 0        4      6      7      8
 +--------+------+------+------+----------------------------
 | rc:i32 | tag  | nptr | kind | fields...
 +--------+------+------+------+----------------------------
           u16    u8     u8
```

| フィールド | 意味 |
|------------|------|
| `rc` | `> 0`: 単一スレッドの参照数 / `< 0`: スレッド共有(アトミック、絶対値が参照数) / `0`: 不死(静的・リージョン) |
| `tag` | 構築子番号 (CON)、またはクロージャのアリティなど kind ごとの意味 |
| `nptr` | 先頭から何個のフィールドが RC 対象のポインタか (255 以上は拡張ヘッダ) |
| `kind` | 下表 |

**フィールドは「ポインタ → 非ボックス値」の順に並べる**(Lean 4 と同じ)。汎用の解放ルーチンは `nptr` 個のポインタを drop して、オブジェクトを解放するだけでよい。

| kind | 内容 |
|------|------|
| `CON` | データ構築子。フィールド |
| `FUN` | クロージャ: `code_ptr`, `arity`, 捕捉変数 |
| `PAP` | 部分適用: 関数オブジェクト + 適用済み引数 |
| `THUNK` | 未評価: `code_ptr` + 捕捉変数 |
| `BLACKHOLE` | 評価中のサンク |
| `IND` | 評価済みサンク → 値へのポインタ |
| `ARRAY` | `len` + ポインタ要素 (boxed 配列) |
| `BYTES` | `len` + 生バイト (非ボックス配列、`ByteArray#` 相当) |
| `BIGINT` | 多倍長整数 (符号 + limb 配列) |
| `FOREIGN` | 外部ポインタ + 解放関数 (`ForeignPtr`) |
| `MUTVAR` | `IORef`/`STRef` |
| `MVAR` | `MVar` (並行性導入後) |

- 引数なし構築子・小さい `Int`/`Char`(-16〜255)は静的オブジェクトを共有し確保しない。
- `Int`/`Double` などは、正格性解析と特殊化で多くの場合 unbox される(boxed は多相位置のみ)。
- 将来の最適化候補: ポインタ下位ビットへの小整数・引数なし構築子の埋め込み(タグ付きポインタ)。

## 2. 公開 ABI (抜粋)

```c
// 確保と解放
void*  cirka_alloc(size_t size);                 // 遅い経路。ヘッダは呼び出し側が初期化
void   cirka_free_object(void* obj);             // RC が 0 になったオブジェクトを解放 (ワークリストで反復)
void   cirka_dec_ref_slow(void* obj);            // 共有(アトミック)オブジェクトの drop
void   cirka_mark_shared(void* obj);             // 到達可能な全オブジェクトを共有マーク

// 評価
void*  cirka_force(void* thunk);                 // サンク評価 (ブラックホール・IND 書き換え)
void*  cirka_apply_n(void* fun, size_t n, void** args);   // 未知関数の適用 (eval/apply)

// 実行時エラー
_Noreturn void cirka_panic(const char* msg, size_t len);   // error / パターン失敗 / <<loop>>

// 起動
int    main(int argc, char** argv);              // ランタイムが提供。cirka_main_closure を実行
```

## 3. アロケータ

- `CIRKA_ALLOC=cirka`(既定): サイズクラス(8 バイト刻み〜256B)ごとのスレッドローカルなフリーリスト + 64KiB ページを `mmap`。
  大きなオブジェクトは `malloc`。Haskell は小さなオブジェクトを大量に確保・解放するため、フリーリストの高速経路は生成コードにインライン化する。
- `CIRKA_ALLOC=system`: すべて `malloc`/`free`。valgrind / ASan と組み合わせて使う。
- 第一段階後半で mimalloc 方式(ページ単位のフリーリスト分割)への置き換えを検討(FEAT-0081、ベンチの計測次第)。

## 4. 関数呼び出し規約 (eval/apply)

- **既知関数の飽和呼び出し**: 直接呼び出し(`tailcc`)。最も多いケース。
- **未知関数の呼び出し**: `FUN` のアリティと引数数を比較し、一致なら直接、少なければ `PAP` 作成、多ければ分割して順に適用。
  頻出の引数数 (1〜4) には専用の `cirka_apply_1..4` を生成コード側で用意。
- クロージャのコードは第1引数に自分の環境ポインタを受け取る。

## 5. I/O

- `IO a` は GHC と同じく `State# RealWorld -> (# State# RealWorld, a #)` の newtype で、`State#` は MIR で消去する。
- ハンドル (`stdin`/`stdout`/`stderr`/ファイル) はランタイムが持つバッファ付き構造体。`hFlush` とプロセス終了時にフラッシュ。
- システムコールは libc 経由(`read`/`write`/`open` …)。

## 6. 例外

- **段階1 (FEAT-0043)**: `error`・`undefined`・パターンマッチ失敗・`exitWith` は `cirka_panic`/`cirka_exit` で
  stdout/stderr をフラッシュしてプロセスを終了(GHC と同じメッセージ形式・終了コード 1)。
- **段階2 (FEAT-0070)**: Itanium C++ ABI のアンワインド (`_Unwind_RaiseException`)。生成コードは `invoke` + landing pad で
  スコープ内の値を drop/release する(Rust の panic=unwind と同じ)。`catch`/`bracket`/`finally`/`throwIO` を実装。

## 7. 起動と終了

1. `main(argc, argv)`: 引数・環境変数 (`CIRKA_*`) を読む。
2. `CIRKA_STACK_SIZE` のスタックを持つスレッドを作成し、ガードページを設定。
3. トップレベルの CAF(定数式)は初回参照時に評価する静的サンクとして配置(RC=0 不死)。
4. `Main.main` の IO アクションを実行。
5. ハンドルをフラッシュし、debug ランタイムなら `CIRKA_LEAK_CHECK=1` で生存オブジェクトを報告(CAF から到達可能なものは除外)。
6. 終了コードを返す。

## 8. debug ランタイムの検査

- 各オブジェクトに確保番号とマジック値を付ける(ヘッダの前に 16 バイトの追加領域)。
- 解放済みオブジェクトはポイズン値で埋め、即座に再利用しない(検疫リスト)。
- dup/drop のたびに RC 値の妥当性 (`rc > 0`) を確認。
- 終了時に生存オブジェクトを kind・tag・確保箇所(コード生成時に埋め込んだ ID)別に集計して表示。
