# 05. ランタイム (`cirka-runtime`)

aCHC と CirkaLang で共有する、Rust で書いた `#![no_std]` の静的ライブラリ `libcirka_rt.a`。依存は libc のみ。
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
| `ARRAY` | `len` + ポインタ要素 (boxed 配列。aCHC の `Cirka.Array`) |
| `BYTES` | `len` + 生バイト (非ボックス配列、`ByteArray#` 相当。aCHC の `Cirka.Array.Unboxed` は 8 バイトの語の列) |
| `BIGINT` | (予約・未使用) 多倍長整数は `BYTES` に `cirka-bigint` の符号化 (符号 1 バイト + u32 limb のリトルエンディアン) で格納する (FEAT-0114) |
| `FOREIGN` | 外部ポインタ + 解放関数 (`ForeignPtr`) |
| `MUTVAR` | `IORef`/`STRef` |
| `MVAR` | 同期オブジェクト: 語 0 が同期構造を指す。tag 0 `MVar`、1 チャネル、2 スレッドのハンドル (§9.4、FEAT-0045) |

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
void*  cirka_array_unique(void* a);              // 一意なら a、共有ならコピー (in-place 更新の前。FEAT-0123)

// 循環回収器 (--cycles=collect。04-memory-model.md §5、FEAT-0126)
void   cirka_cycles_enable(void);                // 生成した cirka_main が最初に呼ぶ
void   cirka_cycle_candidate(void* o);           // o (借用) を根に記録 (根のバッファが参照を 1 つ持つ)。有効でなければ何もしない

// リージョン (04-memory-model.md §6、FEAT-0049)。セルは整数 (アドレス) で指す
void*   cirka_region_new(void);                  // リージョン (FOREIGN オブジェクト、所有)
int64_t cirka_region_free(void* r);              // 終了: 全セルを drop しページを返す (冪等)
int64_t cirka_region_cell(void* r, void* v);     // v (借用) を持つ新しいセル
void*   cirka_region_get(int64_t cell);          // セルの値 (呼び出し側の参照)
int64_t cirka_region_set(int64_t cell, void* v); // セルの値を v (借用) に置き換え、古い値を drop
void*  cirka_bytes_unique(void* b);              // 同上 (BYTES)
void*  cirka_apply_n(void* fun, size_t n, void** args);   // 未知関数の適用 (eval/apply)

// 並行性 (§9、FEAT-0045)。ポインタ引数は借用
void*   cirka_thread_spawn(void* io);            // IO の動作を新しいスレッドで実行。ハンドルを返す
void*   cirka_thread_join(void* h);              // 終了を待って結果を返す (何度でも)
int64_t cirka_thread_id(void);                   // 1 が main のスレッド
void*   cirka_mvar_new(void* v);                 // v を持つ MVar。空は cirka_mvar_new_empty()
void*   cirka_mvar_take(void* m);                // ほかに put / read / try_take / try_put
void*   cirka_mvar_try_take(void* m, void* d);   // 空なら d (dup) を返す。呼び出し側はポインタで比べる
void*   cirka_chan_new(void);                    // ほかに send / recv / try_recv(c, d) / chan_len
int64_t cirka_mutvar_cas(void* r, void* old, void* new); // 値が old なら new に置き換えて 1

// 実行時エラー
_Noreturn void cirka_panic(const char* msg, size_t len);   // error / パターン失敗 / <<loop>>

// 起動
int    main(int argc, char** argv);              // ランタイムが提供。cirka_main_closure を実行
```

## 3. アロケータ

- `CIRKA_ALLOC=cirka`(既定): サイズクラス(8 バイト刻み〜256B)ごとのスレッドローカルなフリーリスト + 64KiB ページを `mmap`。
  大きなオブジェクトは `malloc`。Haskell は小さなオブジェクトを大量に確保・解放するため、フリーリストの高速経路は生成コードにインライン化する。
  ゼロ埋めが必要な大きな BYTES は `calloc` で確保する。OS から得たばかりのメモリは書かずに済み、最初の使用時に一度だけ触れる (FEAT-0068)。
- `CIRKA_ALLOC=system`: すべて `malloc`/`free`。valgrind / ASan と組み合わせて使う。
- 第一段階後半で mimalloc 方式(ページ単位のフリーリスト分割)への置き換えを検討(FEAT-0051、ベンチの計測次第)。

## 4. 関数呼び出し規約 (eval/apply)

- **既知関数の飽和呼び出し**: 直接呼び出し(`tailcc`)。最も多いケース。
- **未知関数の呼び出し**: `FUN` のアリティと引数数を比較し、一致なら直接、少なければ `PAP` 作成、多ければ分割して順に適用。
  頻出の引数数 (1〜4) には専用の `cirka_apply_1..4` を生成コード側で用意。
- クロージャのコードは第1引数に自分の環境ポインタを受け取る。

## 5. I/O

- `IO a` は Core では `Unit# -> a` の newtype (`Unit#` は GHC の `State# RealWorld` にあたる消去されるトークン。ADR-032)。
- 標準出力はランタイムのバッファに書き、`hFlush`・標準入力の読み込み前・stderr への書き込み前・プロセス終了時にフラッシュする。
  標準入力もバッファ付きで、`cirka_read_byte` / `cirka_peek_byte` (`isEOF`) で読む。
- ファイルは H2 では丸ごとの読み書きだけ (`cirka_file_read` / `cirka_file_write`: `readFile`・`writeFile`・`appendFile`)。
  失敗すると GHC の未捕捉 `IOError` と同じ形式 (`prog: PATH: openFile: does not exist (No such file or directory)`) で終了コード 1。
  任意のハンドル (`openFile`・`hGetLine` …) は例外 (FEAT-0040) の後で扱う。
- CirkaLang の標準ライブラリ (FEAT-0213、`stdio.rs`) は終了しない関数を使う: `cirka_fs_open(path, mode)` / `cirka_fd_close` / `cirka_fs_remove` は
  記述子か 0、失敗は `-errno`。`cirka_fd_read(fd, max)` (`max < 0` で最後まで。記述子 0 は標準入力のバッファを通す) は文字列を返し、
  失敗は空の文字列と `errno` (`cirka_errno` で読む。成功時は 0)。`cirka_fd_write(fd, s)` は記述子 1・2 への書き込みの前に標準出力を書き出す。
  ほかに `cirka_strerror` (ロケールに依存しない固定の文言)、`cirka_stdin_eof` / `cirka_read_line`、`cirka_getenv` / `cirka_env_has`、
  `cirka_clock_ns` (0 は実時間、1 は単調時計)、`cirka_cstring_new` / `cirka_cstring_peek` (`withCString` / `peekCString`)。
  MIR インタプリタは同じ関数を自前で (ファイルは自身の記述子の表で) 実装する。
- `IORef` は `MUTVAR` オブジェクト (1 ワードの RC 管理フィールド)。`cirka_mutvar_new` / `read` (dup して返す) / `write`
  (新しい値を dup し、古い値を drop)。外部関数のポインタ引数は借用なので、所有権の受け渡しはランタイム側で数える。
- システムコールは libc 経由(`read`/`write`/`open` …)。
- 生メモリ (FEAT-0116、`Foreign.Storable`): `cirka_mem_read(addr, kind)` / `cirka_mem_write(addr, kind, val)` (kind: 0 `int8_t` 〜 6 `int64_t`)、
  `cirka_mem_read_f` / `cirka_mem_write_f` (`float` / `double`)。アドレスは検査しない (C・GHC と同じ)。非整列アクセスを許す。

## 6. 例外

- **段階1** (aCHC: FEAT-0113、CirkaLang: FEAT-0212): `error`・`undefined`・パターンマッチ失敗・`exitWith` は `cirka_panic`/`cirka_exit` で
  stdout/stderr をフラッシュしてプロセスを終了(GHC と同じメッセージ形式・終了コード 1)。
  aCHC は `error` の呼び出しを型検査で `cirkaErrorCalledAt "<file>:<行>:<列> in main:<モジュール>"` に置き換え、
  GHC の `CallStack (from HasCallStack):` の行を出す。`head []` などライブラリ内部の呼び出し履歴 (GHC の base の行番号) は出さない。
- **段階2 (FEAT-0040)**: Itanium C++ ABI のアンワインド (`_Unwind_RaiseException`)。生成コードは `invoke` + landing pad で
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

## 9. 並行性 (FEAT-0045)

OS のスレッド (pthread) を 1 対 1 で使う。グリーンスレッドは持たない (多数の接続は FEAT-0046 のイベントループで扱う)。

### 9.1 スレッドの生成と終了

- `cirka_thread_spawn(f)`: `f` は IO の動作 (トークンを 1 つ取るクロージャ。借用)。`f` を dup して共有マークし (§9.3)、
  `CIRKA_THREAD_STACK_SIZE` (既定 64M) のスタックを持つ切り離された (detached) スレッドを作る。新しいスレッドは生成コードの C ABI の入口
  `cirka_call_1(f, token)` で `f` を適用し、結果を共有マークしてハンドルに置く。ランタイムからクロージャを呼ぶ唯一の入口で、
  生成コードは `tailcc` の `cirka_apply_1` を包んで常に出力する。トークンは `main` に渡すものと同じ形の箱 (CON、語 1 つ)。
- ハンドルは同期オブジェクト (§9.4、`MVAR` の tag 2)。`cirka_thread_join(h)` は結果が置かれるまで待ち、結果を dup して返す (何度でも)。
- スレッド番号: `main` を走らせるスレッドが 1、`spawn` が作るスレッドは 2 から順に。ランタイムが作っていないスレッド
  (C から呼ばれた外部のスレッド・単体テスト) は最初に番号を使うときに割り当てる。
- 最初の `spawn` でプロセスはマルチスレッド (MT) の状態になる。この時点で評価済みの CAF の値を共有マークし、以後に評価された CAF の値もマークする。
- `main` が終わるとプロセスは終了する (Haskell と同じ。ほかのスレッドを待たない)。ただし結果を置き終えたスレッドの後始末
  (ハンドルの解放・統計の合算、数命令) は待つので、join したスレッドの確保と解放は `CIRKA_STATS` に数えられる。
  まだ走っているスレッドがあれば、循環回収の終了処理とリークの検査は行わない。
- 各スレッドは自分の代替シグナルスタックを持ち、スタックの範囲を TLS に置く。スタックあふれはどのスレッドでも `stack overflow` と報告する。
- スレッドの中の `error`・パニックは、アンワインド (FEAT-0040) までは今と同じくプロセス全体を終了する。

### 9.2 スレッドごとの状態

- ランタイムのスレッドごとの状態 (アロケータのフリーリスト・統計・スレッド番号・スタックの範囲) は 1 つの TLS ブロックに置く。
  安定版の Rust の `no_std` には `#[thread_local]` がないので、`global_asm!` で `.tbss` にブロックを定義し、`asm!` で
  `fs:0` + `@tpoff` (local-exec) の番地を得る。ランタイムは実行ファイルに静的にリンクされるので local-exec でよい。
- フリーリストはスレッドごと。別のスレッドが解放したオブジェクトは解放したスレッドのフリーリストに入る。スレッドの終了時に
  フリーリストを大域のプールへ返し、ページを切り出す前にプールから取る。ページの切り出しは大域のスピンロックの中で行う。
  確保の高速経路 (フリーリストから 1 つ取る) はレジスタを退避しない葉の関数で、初期化・補充・大きなオブジェクトは別の関数に置く
  (遅い経路をインライン展開すると、退避するレジスタが増えて高速経路が遅くなる)。
- 確保と解放は TLS ブロックの番地を `fs:0` から 1 回読んで求め、MT かどうかを見ない。単一スレッドの間だけフリーリストを大域に置き、
  最初の `spawn` でスレッドへ移す方式も試したが、高速経路に MT の判定の分岐が入り binary-trees で 13% 遅くなった
  (TLS だけの方式と大域だけの方式の差は 1% 未満。FEAT-0045)。
- 統計 (`CIRKA_STATS`) はスレッドごとに数え、スレッドの終了時に大域の合計に足す (`max_live` はスレッドごとの最大の和で、目安)。
- 標準入出力のバッファと debug ランタイムの生存表は、MT のときだけロックを取る。単一スレッドのプログラムはロックの費用を払わない。
- 循環回収 (`--cycles=collect`) は MT になった後は候補を記録せず、回収もしない (共有された循環はリークになる)。

### 9.3 共有マークとアトミック RC

- `cirka_mark_shared(o)`: `o` から到達できる、RC が正のオブジェクトすべての RC を負にする (04-memory-model.md §2.4)。
  作業スタックは `malloc` で伸ばし、深い構造でも C のスタックを使わない。RC が 0 (不死) と負 (共有済み) のオブジェクトの先は辿らない。
  同期オブジェクトは中身も辿る。
- マークする時点は、値がほかのスレッドから見えるようになる直前: `spawn` の引数、共有された同期オブジェクトに入れる値、
  共有されたサンクの評価結果、共有された `MUTVAR` に書く値、MT のときの CAF の値、スレッドの結果。
  マークするのはまだ誰とも共有していない値なので、マーク自体は非アトミックでよい。
- 共有されたオブジェクトの dup はアトミックな加算 (Relaxed)、drop はアトミックな減算 (AcqRel) で、0 にしたスレッドが解放する。
  生成コードの高速経路は RC の符号を見て、負ならランタイムを呼ぶ。
- 共有されたオブジェクトは一意 (`rc == 1`) にならないので、再利用と配列のその場の更新はコピーになる (Lean 4 と同じ)。
- 生成コードとランタイムがヘッダ (RC と kind) を読むロードはアトミックロードにする (生成コードは `unordered`、ランタイムは Relaxed)。
  x86-64 では普通の `mov` で、共有されたヘッダをほかのスレッドが書き換えている間の読み込みをデータ競合にしない。
  `unordered` は LLVM がロードを読み直さない (分岐の判断と値が食い違わない) ことだけを保証し、重複の除去やループ外への移動は妨げない。
- 生成コードは正の RC への書き込みとヘッダの初期化も `unordered` のアトミックストアにする。共有されていないオブジェクトに触れるのは
  1 スレッドだけなので順序の保証は要らず、命令も同じ `mov` だが、LLVM は普通のストアの値を後のアトミックロードへ渡さない
  (store-to-load forwarding をしない) ので、ロードだけをアトミックにすると、書いたばかりの RC やヘッダを読み直す命令が残る。
  ランタイムの中の正の RC への書き込みは非アトミックのまま。
- 不変条件: 共有されたオブジェクトから到達できるオブジェクトは共有か不死。値をオブジェクトに書き込む操作は、書き込み先が共有なら値を共有マークする
  (同期オブジェクト・`MUTVAR`・サンクの更新に加え、可変の借用を通した配列への格納 `array_set_shared` も生成コードで RC の符号を見る。
  ほかのスレッドから受け取った一意な配列は RC が負のまま)。

### 9.4 同期オブジェクト (`MVAR`)

- kind `MVAR` のオブジェクトの語 0 は、`malloc` した同期構造 (pthread の mutex と条件変数、中身) を指す。
  tag 0 は `MVar` (空か値 1 つ)、tag 1 はチャネル (上限のない FIFO)、tag 2 はスレッドのハンドル。
- `MVar`: `cirka_mvar_new(v)`・`cirka_mvar_new_empty()`、`take` (空なら待つ)、`put` (値があれば待つ)、`read` (待って dup)、
  `try_take(m, d)` (空なら `d` を dup して返す)、`try_put` (値があれば 0)。チャネル: `cirka_chan_new`・`send` (待たない)・`recv` (空なら待つ)・
  `try_recv(c, d)` (空なら `d`)・`chan_len`。MIR と Core は null を扱わないので、「なし」は呼び出し側が渡した既定値とのポインタ比較 (`ptr_eq`) で表す。
- 同期構造の mutex と条件変数は pthread のもの (待つスレッドは眠る)。同期オブジェクトが解放されると中身を drop する。
- 排他制御 (`Mutex a`) は `MVar a` で表す (取り出して施錠、戻して解錠。Haskell の慣用)。言語の API は FEAT-0240 (CirkaLang)・FEAT-0162 (aCHC)。
- 外部関数のポインタ引数は借用なので、`put`・`send` は値を dup してから入れる。同期オブジェクトが共有されていれば値を共有マークする。

### 9.5 サンクの評価

- 共有されたサンク (RC が負) と MT のときの CAF は、kind を CAS で `THUNK` → `BLACKHOLE` にしたスレッドだけが評価する。
  共有されていないサンクは今までどおり普通の書き込みで `BLACKHOLE` にする (アトミックな読み書きの命令は x86-64 では普通の `mov`)。
- 評価するスレッドは、コードのポインタを読んだ直後に語 0 を持ち主の印 (最上位ビット + そのスレッドの TLS ブロックの番地) に書き換える。
  TLS ブロックの番地は走っているスレッドの間で一意で、MT になる前後で変わらず、すでに手元にある (統計の更新に使う) ので、書き込みは 3 命令で済む。
  評価の後、サンクが共有されていれば (評価中に共有されたものも) 結果を共有マークし、語 0 に置き、kind を `IND` にする (Release)。
  `IND` を読むスレッドは kind を Acquire で読む。
- `BLACKHOLE` に出会ったスレッドは、MT でなければ `<<loop>>`。MT なら、印が自分のものなら `<<loop>>`、
  ほかのスレッドのもの (または印がまだない) なら `IND` になるまで待つ (スピンの後 `sched_yield`)。
  スレッドをまたぐ循環は待ち続ける (GHC は GC で検出するが、GC がないので検出しない)。
- 単一スレッドの性能を保つため、`cirka_force` 自身はレジスタを退避しない。RC が正のサンクは種別を書き換えて評価の関数へ跳び、
  生き残る間接参照 (RC が 2 以上か 0) は呼び出しなしで値を返す。RC が 0 か負のサンク・`BLACKHOLE`・死ぬか共有された間接参照は別の関数で扱う。

### 9.6 `MUTVAR` (`IORef`)

- 共有された `MUTVAR` の読み書きは、語 0 を「使用中」の印 (奇数の番地 1) と CAS で交換して取り、操作の後に書き戻す (語 0 自体をスピンロックにする)。
  読むスレッドが dup する前に、書くスレッドが古い値を解放することがない。共有されていなければこれまでどおり。
- `cirka_mutvar_cas(r, old, new)`: 値が `old` と同じオブジェクトなら `new` に置き換えて 1、違えば 0。
  `atomicModifyIORef` はライブラリで読み出しと CAS の繰り返しとして書く (GHC の `atomicModifyIORef'` と同じ。FEAT-0162・FEAT-0240)。

### 9.7 ThreadSanitizer と MIR インタプリタ

- `--runtime=tsan`: ランタイムを `-Zsanitizer=thread` で、生成コードを `sanitize_thread` 属性付きでコンパイルし、`clang -fsanitize=thread` でリンクする。
  安定版の Rust には sanitizer がないので、この版のランタイム (`target/rt-tsan`) のビルドだけ `RUSTC_BOOTSTRAP=1` と
  `-Cunsafe-allow-abi-mismatch=sanitizer` (`core` は計装されない) を使う。
- テストのヘッダ `tsan: 1` の MIR 実行テストは、debug・release に加えて TSan 版 (既定のアロケータと `CIRKA_ALLOC=system`) でも実行し、
  TSan の報告があれば失敗にする (10-testing.md)。
- 高エントロピーの ASLR (`vm.mmap_rnd_bits=32`、Ubuntu 24.04 の既定) では、TSan (LLVM 18) は `personality(2)` で ASLR を切って
  自分を再実行する。Docker の既定の seccomp プロファイルはこれを禁じるので、開発用コンテナは既定のプロファイルに
  `personality(ADDR_NO_RANDOMIZE)` を許す規則を 1 つだけ足したプロファイル (`docker/seccomp.json`) で動かす (FEAT-0084)。
  ASLR を切るのは TSan で計装したプロセスだけ。テストは TSan が動くか試し、動かない環境では TSan の構成を飛ばすが、
  `scripts/ci.sh` は TSan が動かなければ失敗する。
- MIR インタプリタはスレッドを扱わない (`spawn` は実行時エラー)。`MVar`・チャネルは単一スレッドの意味で実装し、
  待つことになれば `thread blocked indefinitely in an MVar operation` で終了する (GHC と同じ文言)。
