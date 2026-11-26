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
| `rc` | `> 0`: 単一スレッドの参照数 / `< 0`: スレッド共有(アトミック、絶対値が参照数) / `0`: 不死(静的・リージョン)。生成コードの静的なオブジェクト (文字列リテラル・引数のない構築子・静的な大域変数) は読み取り専用の定数に置く (FEAT-1020) ので、ランタイムは RC 0 のオブジェクトのヘッダにもフィールドにも書き込まない (`dup` / `drop` / 共有マーク / 循環の検査は RC 0 を飛ばす) |
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
void   cirka_free_object(void* obj);             // RC が 0 になったオブジェクトを解放 (ワークリストで反復。
                                                 // ポインタのフィールドがなければすぐ解放、FEAT-1014)
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

// バイト列の操作 (FEAT-0095)。範囲は入口で 1 回検査し、外れていれば panic ("index out of bounds")
int64_t cirka_bytes_copy(void* dst, int64_t off, void* src, int64_t i, int64_t n); // memmove。dst は一意 (書き込み中)
int64_t cirka_bytes_index(void* s, int64_t b, int64_t i, int64_t j);  // memchr。[i, j) で最初の b の位置、なければ -1
int64_t cirka_bytes_eq(void* s, int64_t i, void* t, int64_t j, int64_t n); // memcmp。等しければ 1
void*  cirka_bytes_resize(void* b, int64_t n); // b を消費し長さ n の BYTES を返す (内容は先頭から保つ。伸ばした部分は 0)。一意な malloc の塊は realloc でその場で伸縮 (FEAT-0096)
void*  cirka_bytes_from(const void* p, size_t n); // p の n バイトの写しの BYTES。n が 0 ならランタイムの静的な空文字列 (不死。読み込みの空の結果は確保しない、FEAT-1002。MIR インタプリタも同じ外部関数の空の結果を数えない)。64 バイトまでは両端から重ねた 16 (8・4・1) バイトの読み書きで写し、memcpy を呼ばない (FEAT-1022)
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

// イベントループとソケット (§10、FEAT-0046)。ポインタ引数は借用、失敗は -errno
void*   cirka_loop_new(void);                    // 呼んだスレッドのループ。ほかに watch / unwatch / events / timer / cancel / run / stop
void*   cirka_loop_current(void);                // run 中のループ (コールバックはループを捕捉せずにこれで得る)
void*   cirka_loop_remote(void* l);              // 受け口。cirka_remote_post(rm, io) はどのスレッドからでも呼べる
int64_t cirka_loop_blocking(void* l, void* work, void* done); // work をスレッドプールで実行し、done r をループで呼ぶ
int64_t cirka_net_listen(void* host, int64_t port, int64_t backlog); // ほかに accept / connect / send / resolve …

// 実行時エラー
_Noreturn void cirka_panic(const char* msg, size_t len);   // error / パターン失敗 / <<loop>>

// 起動
int    main(int argc, char** argv);              // ランタイムが提供。cirka_main_closure を実行
```

## 3. アロケータ

- `CIRKA_ALLOC=cirka`(既定): サイズクラス(8 バイト刻み〜256B)ごとのスレッドローカルなフリーリスト + 64KiB ページを `mmap`。
  大きなオブジェクトは `malloc`。Haskell は小さなオブジェクトを大量に確保・解放するため、フリーリストの高速経路は生成コードにインライン化する。
  ゼロ埋めが必要な大きな BYTES は `calloc` で確保する。OS から得たばかりのメモリは書かずに済み、最初の使用時に一度だけ触れる (FEAT-0068)。
- **大きな配列の巨大ページ** (FEAT-1011、ADR-116): 2 MiB 以上の配列は、2 MiB 境界に揃った内側の範囲 (先頭の切り上げから
  末尾の切り下げまで) に `madvise(MADV_HUGEPAGE)` を付け、透過的な巨大ページ (THP) に置く。対象は 0 で埋めて確保するオブジェクト
  (`bytes_new`: unboxed の配列。同じ経路で作る連結・切り出しの文字列も含む) と箱の配列 (`cirka_array_new` / `cirka_array_unique`)。
  ページフォールトは 512 分の 1 になり、大きな配列を走査するループの TLB の外れも減る。`realloc` で伸ばすバッファ (`Builder`) や
  ほかの `malloc` の塊には付けない: 一度書くだけのバッファでは、最初のフォールトでカーネルが 2 MiB を丸ごと 0 で埋めてキャッシュを
  追い出す損が、TLB の得より大きい (すべての大きなオブジェクトに付けると cli-grep と string-split が 6 % 遅くなった)。
  システムの THP の設定が `madvise` (Ubuntu の既定) でも巨大ページになり、`always` では変わらず、`never` や THP のないカーネルでは何もしない (`madvise` の失敗は無視する)。
  代償: 触れた 2 MiB の塊は丸ごと物理メモリを使う (巨大なゼロの配列を疎に使うと常駐メモリが増える)、巨大ページが空いていないと
  カーネルが最初のフォールトでメモリを詰め直すことがある。`CIRKA_THP=0` で無効にする (確保のたびに見ず、最初の大きな確保で一度だけ読む)。
  glibc が先頭に置く塊のヘッダのページは確保の時点で触れられているので、先頭の 2 MiB の塊は小さなページのまま。
- `CIRKA_ALLOC=system`: すべて `malloc`/`free`。valgrind / ASan と組み合わせて使う。
- 第一段階後半で mimalloc 方式(ページ単位のフリーリスト分割)への置き換えを検討(FEAT-0051、ベンチの計測次第)。

## 4. 関数呼び出し規約 (eval/apply)

- **既知関数の飽和呼び出し**: 直接呼び出し(`tailcc`)。最も多いケース。
- **未知関数の呼び出し**: `FUN` のアリティと引数数を比較し、一致なら直接、少なければ `PAP` 作成、多ければ分割して順に適用。
  頻出の引数数 (1〜4) には専用の `cirka_apply_1..4` を生成コード側で用意。`cirka_apply_<n>` の入口は `FUN` で引数の数が合う
  場合だけを調べて本体へ末尾呼び出しし (レジスタを退避しない)、ほかの場合は `cirka_apply_<n>_slow` (インライン化しない) に任せる
  (FEAT-1015)。
- クロージャのコードは第1引数に自分の環境ポインタを受け取る。

## 5. I/O

- `IO a` は Core では `Unit# -> a` の newtype (`Unit#` は GHC の `State# RealWorld` にあたる消去されるトークン。ADR-032)。
- 標準出力はランタイムのバッファに書き、`hFlush`・標準入力の読み込み前・stderr への書き込み前・プロセス終了時にフラッシュする。
  標準入力もバッファ付きで、`cirka_read_byte` / `cirka_peek_byte` (`isEOF`) で読む。
- ファイルは H2 では丸ごとの読み書きだけ (`cirka_file_read` / `cirka_file_write`: `readFile`・`writeFile`・`appendFile`)。
  失敗すると GHC の未捕捉 `IOError` と同じ形式 (`prog: PATH: openFile: does not exist (No such file or directory)`) で終了コード 1。
  任意のハンドル (`openFile`・`hGetLine` …) は例外 (FEAT-0040) の後で扱う。
- CirkaLang の標準ライブラリ (FEAT-0213、`stdio.rs`) は終了しない関数を使う: `cirka_fs_open(path, mode)` / `cirka_fd_close` / `cirka_fs_remove` は
  記述子か 0、失敗は `-errno`。`cirka_fd_read(fd, max)` (`max < 0` で最後まで。記述子 0 は標準入力のバッファを通す。最後まで読むときは通常のファイルなら `fstat` の大きさの
  塊に BYTES の頭と長さの分を空けて直接読み、足りなければ `realloc` で伸ばし、読み終えた塊をそのまま BYTES にする: FEAT-0096) は文字列を返し、
  失敗は空の文字列と `errno` (`cirka_errno` で読む。成功時は 0)。`cirka_fd_write(fd, s)` は記述子 1・2 への書き込みの前に標準出力を書き出す。
  ほかに `cirka_strerror` (ロケールに依存しない固定の文言)、`cirka_stdin_eof` / `cirka_read_line`、`cirka_getenv` / `cirka_env_has`、
  `cirka_clock_ns` (0 は実時間、1 は単調時計)、`cirka_random_u64` (`getrandom` の 64 ビット。CirkaLang v0.0.7 の `Std.Random`)、
  `cirka_cstring_new` / `cirka_cstring_peek` (`withCString` / `peekCString`)。
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
2. `CIRKA_STACK_SIZE` のスタックを持つスレッドを作成し、ガードページを設定。最初のスレッドはそのスレッドを待つだけで、
   すべてのシグナルをブロックしてから作る。作ったスレッドは起動時のマスクに戻してからプログラムを実行する (§11.3)。
3. トップレベルの CAF(定数式)は初回参照時に評価する静的サンクとして配置(RC=0 不死)。リテラルの定数は CAF にせず、Core の簡約が使う箇所で値に置き換える (リテラルの構築子は分解する箇所で。FEAT-1004、06-ir.md §1.3)。定数だけを受け取る構築子の定義はその構築子の静的なオブジェクト (RC=0 不死) にし、`force` も確保もしない (FEAT-1006、06-ir.md §2.2)。
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
  生成コードは `tailcc` の `cirka_apply_1` を包んで常に出力する。トークンは静的な不死のオブジェクト 1 つ (CON、語 1 つ、
  `rc` 0。FEAT-1014、ADR-118): スレッドの開始やイベントループのコールバックの呼び出しごとに確保せず、動作がそれを `drop` しても
  何もしない (静的な空文字列と同じく、再利用もされない)。
- ハンドルは同期オブジェクト (§9.4、`MVAR` の tag 2)。`cirka_thread_join(h)` は結果が置かれるまで待ち、結果を dup して返す (何度でも)。
- スレッド番号: `main` を走らせるスレッドが 1、`spawn` が作るスレッドは 2 から順に。ランタイムが作っていないスレッド
  (C から呼ばれた外部のスレッド・単体テスト) は最初に番号を使うときに割り当てる。
- 最初の `spawn` でプロセスはマルチスレッド (MT) の状態になる。この時点で評価済みの CAF の値を共有マークし、以後に評価された CAF の値もマークする。
- `main` が終わるとプロセスは終了する (Haskell と同じ。ほかのスレッドを待たない)。ただし結果を置き終えたスレッドの後始末
  (ハンドルの解放・統計の合算、数命令) は待つので、join したスレッドの確保と解放は `CIRKA_STATS` に数えられる。
  まだ走っているスレッドがあれば、循環回収の終了処理とリークの検査は行わない (リークの検査を求められていれば
  `cirka: leak check skipped: N threads still running` と知らせる。テストはこれを失敗にする)。
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
- `errno` は C ライブラリのもの (libc の関数とシステムコールの直接の呼び出し (FEAT-1007) が同じ値を読み書きする) で、そのアドレスはスレッドの間は変わらないので、スレッドごとに最初の使用で 1 度だけ `__errno_location` に聞いて TLS ブロックに置く。以後の読み書き (`cirka_errno`・空の受信の `EAGAIN` など) はブロックからの読み込みだけで、C ライブラリの関数 (別のページのコード) を呼ばない (FEAT-1021)。受信のバッファ・実行中のループ・`errno` のアドレスは、受信が 1 つのキャッシュラインで読めるようにブロックの中で並べる。
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
- 同期構造の mutex と条件変数は pthread のもの。同期オブジェクトが解放されると中身を drop する。
- 待つスレッドは、mutex を放して同期構造の **変化の回数** (原子的な値。起こす側が mutex の下で増やす) をしばらく見張ってから眠る
  (FEAT-0099): 初めの 5 µs は pause の合図で回り、20 µs まではプロセッサを譲りながら見る。変わったら mutex を取り直して条件を
  確かめ直し、変わらなければ mutex の下で回数を確かめてから条件変数で眠る (最後に見てから眠るまで mutex を持つので、変化を
  見落とさない)。起こす側は mutex を放してから `pthread_cond_broadcast` する (起きたスレッドがすぐ mutex で待たされない)。
  別のプロセッサで動く相手の応答は数 µs で来るので、往復する 2 つのスレッドは眠らずに済む。
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

## 10. 非同期 I/O (FEAT-0046)

多数の接続は、スレッドごとのイベントループ (epoll) とコールバックで扱う (ADR-087)。グリーンスレッドも言語の `async` も持たず、
ライブラリがコールバックの上に継続渡しの API を作る (CirkaLang は FEAT-0241)。コールバックは IO の動作 (トークンを 1 つ取るクロージャ) で、
結果は捨てる。ポインタ引数は借用で、ループが dup して持ち、呼ぶときに手放す。

### 10.1 イベントループ

- `cirka_loop_new()`: ループ (kind `FOREIGN`。語 0 は `malloc` した状態、語 1 は後始末の関数)。作ったスレッドが持ち主で、
  `watch`・`unwatch`・`events`・`timer`・`cancel`・`blocking`・`run`・`stop`・`remote` は持ち主のスレッドだけが呼べる (ほかのスレッドから呼ぶと
  プログラムを終了する)。ループに入れたコールバックは共有マークしない (単一スレッドのサーバが RC のアトミック操作と再利用の喪失を払わない)。
  そのためループのオブジェクト自体をほかのスレッドへ渡してはならない (CirkaLang では `Send` でない型にする。FEAT-0240)。
- `cirka_loop_current()`: このスレッドで `cirka_loop_run` が動いているループ (入れ子なら内側。新しい参照)。なければプログラムを終了する。
  コールバックはループを捕捉せずにこれで得る: ループはコールバックを持つので、捕捉すると参照の循環になり、ループを捨てても解放されない。
- `cirka_loop_watch(l, fd, events, cb)`: `fd` が `events` (1 読める、2 書ける) になったら 1 度だけ `cb` を呼ぶ。
  続けて待つならコールバックの中でまた `watch` する。待ちは fd ごとに 1 つで、2 度目は前の待ちを置き換える (前のコールバックは呼ばずに解放)。
  起きた事象はコールバックの中で `cirka_loop_events(l)` で読む (1 読める、2 書ける、4 エラーか切断)。エラーと切断は待っていた事象も
  立てて知らせる (読み書きが失敗か終わりを返すので、コールバックはふだんどおり読み書きすればよい。libuv と同じ)。0 か `-errno`。
- **登録 (FEAT-1001、ADR-111).** 待った fd は、閉じるか `unwatch` するまで epoll に入れたままにする (読み書きの両方向・
  エッジトリガー・`EPOLLRDHUP`・`EPOLLPRI`)。カーネルは状態が変わるたびに 1 度知らせ、ループは fd ごとに「準備ができているかもしれない」
  向き (`ready`) を覚える。知らせた事象を待つコールバックがなければ覚えるだけで、待ちが来たときに使う。
- epoll のデータには fd と登録の番号を入れる。番号は、記述子オブジェクト (§10.3) の待ちならオブジェクトの通し番号、fd の番号での
  待ち (`cirka_loop_watch`) なら待つたびに新しい番号。1 回の `epoll_wait` で得た事象のうち、番号が今の登録と合わないもの
  (コールバックが先に閉じて同じ番号が再利用された fd の古い事象) は捨てる。
- 待ちがシステムコールを使うのは、fd がまだその番号で登録されていないとき (`EPOLL_CTL_ADD`。閉じて番号が再利用された fd は
  `EPOLL_CTL_MOD` が `ENOENT` で失敗するので `ADD` でやり直す) と、登録済みでも待つ向きが `ready` のとき (`EPOLL_CTL_MOD`。
  どちらもその時点の状態を次の `epoll_wait` で知らせる) だけ。`ready` は、§10.3 のソケットの操作が「準備ができていない」と分かったときに
  落とす (`drain`): `EAGAIN` を受けたとき、ストリームの読み込みが求めたより少なかったとき (受信バッファを空にした。ただし
  終わり・エラー・切断・緊急データを知らされた後は信用しない: 読み込みはそこで止まる)。落ちていれば待ちはシステムコールなしで次の
  エッジを待つ。それ以外の操作 (signalfd など) の後の待ちは、`MOD` でカーネルに聞き直す (前の 1 度だけの待ちと同じ数のシステムコール)。
  登録済みで `ready` でない記述子オブジェクトを、コールバックも時限もない枠で時限なしに待つ場合 (コールバックが同じ記述子をまた待つ。
  最も多い) は、枠に書くだけの短い経路にし、ほかは関数の外に出す (FEAT-1014)。
- `cirka_loop_unwatch(l, fd)`: 待ちを取り消す (コールバックは呼ばずに解放、epoll から外す)。待っている fd を閉じる前に呼ぶ:
  閉じた fd は epoll から黙って消え、待ちが残るとループが終わらない。待っていない fd は閉じてよい (登録はカーネルが消す)。
- `cirka_loop_timer(l, ms, cb)`: `ms` ミリ秒後に `cb` を呼ぶ。番号 (正) を返し、`cirka_loop_cancel(l, id)` で取り消せる (取り消したら 1)。
  時刻は `CLOCK_MONOTONIC`。タイマーは番号の表 (スラブ。番号に世代を含むので、使い回した要素を古い番号で取り消すことはない) と
  期限の二分ヒープで持ち、取り消しは O(1) (ヒープに残った要素は取り出すときに飛ばす)。期限が同じなら作った順に呼ぶ。
- `cirka_loop_blocking(l, work, done)`: ブロックする処理 `work` (IO の動作) をランタイムのスレッドプールで実行し、結果 `r` について
  `done r` (IO の動作) をループのスレッドで呼ぶ。`work`・`done`・`r` はスレッドを越えるので共有マークする。
  プールは最初の `blocking` で作る (`CIRKA_POOL_THREADS`、既定は CPU 数。プロセスはここで MT になる)。
  待ち行列に入った仕事から終わるまでを「動いているスレッド」に数える (終了時のリーク検査は、動いているスレッドがあれば行わない)。
  結果を受け口に入れた後の仕事は §9.1 の終わりかけのスレッドと同じに数え、プロセスの終了はそれを待つ。
  プールのスレッドは終わらないので、仕事を 1 つ終えるたびに確保の統計を合計に足す。
- `cirka_loop_remote(l)`: ループの受け口 (kind `FOREIGN`)。`cirka_remote_post(rm, cb)` はどのスレッドからでも呼べ、`cb` を共有マークして
  受け口の待ち行列 (pthread の mutex) に入れ、eventfd でループを起こす。ループがもうなければ `-EPIPE`。受け口の状態はループと別に数え
  (アトミックな参照数)、ループ・受け口のオブジェクト・実行中の `blocking` の仕事が持つ。
  受け口のオブジェクトが 1 つでも生きている間は、まだ post が来うるのでループは終わらない (チャネルの送り手と同じ。libuv の async
  ハンドルにあたる)。最後の受け口が解放されるとループを起こし、ループは待ち行列を実行してから終わりを判断する
  (受け口の数を待ち行列より先に読むので、解放の前の post を取りこぼさない)。ループのスレッド自身が受け口を持ち続けると
  ループは終わらないので、受け口はほかのスレッドへ渡して手放す。
- `cirka_loop_run(l)`: 1 回の繰り返し (ターン) で、受け口の待ち行列 (post と `blocking` の完了)、期限の来たタイマー (その時点のもの。
  コールバックが足したタイマーは期限が来ていても次のターン)、epoll の事象 (最大 256) の順にコールバックを呼ぶ。待ち・タイマー・
  待ち行列・実行中の `blocking`・受け口がなくなると 0 を、`cirka_loop_stop(l)` が呼ばれたらそのターンの終わりで 1 を返す
  (残った待ちとタイマーは次の `run` に引き継ぐ)。同じループの入れ子の `run` はプログラムを終了する。
- ループのオブジェクトが解放されると、待ちとタイマーのコールバックを呼ばずに解放し (持ち主以外のスレッドで解放されたときは、
  データ競合を避けるため解放せずリークさせる)、受け口を閉じて待ち行列の動作 (共有済み) を解放し、epoll を閉じる。

### 10.2 ソケットと記述子

- TCP (IPv4・IPv6 の数値アドレス。空のホストは IPv4 の任意のアドレス): `cirka_net_listen(host, port, backlog)`・`cirka_net_accept(fd)`・
  `cirka_net_connect(host, port)`・`cirka_net_connect_result(fd)`・`cirka_net_local_port(fd)`・`cirka_net_set_nodelay(fd, on)`・
  `cirka_net_shutdown(fd, how)`。作るソケットは非ブロッキングで close-on-exec、待ち受けは `SO_REUSEADDR` (`backlog` が 0 以下なら 4096。
  カーネルの `somaxconn` で切り詰められる)。失敗は `-errno` (Std の規約、FEAT-0213)。`accept` は待っている接続がなければ `-EAGAIN`。
  `connect` は完了を待たずに fd を返し (`EINPROGRESS`)、書けるようになったら `connect_result` で結果 (0 か `-errno`) を読む。
- 名前解決 `cirka_net_resolve(host)` (`getaddrinfo` の最初のアドレスの文字列。分からなければ空で `errno` が `ENOENT`) はブロックするので
  `blocking` の中で呼ぶ。
- 読み書き: `cirka_fd_read(fd, max)` は非ブロッキングの記述子でデータがなければ空を返し `errno` が `EAGAIN` (11)、終わりなら空で `errno` が 0。
  `cirka_fd_write_from(fd, data, off)` は `data` の `off` バイト目から 1 回の `write` で書けるだけ書き、書いたバイト数か `-errno` を返す。
  ソケットには `cirka_net_send(fd, data, off)` (同じ意味の `send(MSG_NOSIGNAL)`) を使う: 相手が閉じた接続への書き込みは `SIGPIPE` で
  プロセスを終わらせず `-EPIPE` か `-ECONNRESET` を返す。ランタイムは `SIGPIPE` の扱いを変えない (閉じたパイプへ書いた CLI は C の
  プログラムと同じく黙って終わる)。ソケットかどうかは型で分かるので、`write` で試して `ENOTSOCK` なら `send` するような余分な
  システムコールは払わない。`cirka_fd_set_nonblocking(fd, on)`。`cirka_strerror` はネットワークの errno の文言も返す。
- `cirka_fd_limit_raise()`: `RLIMIT_NOFILE` の soft を hard (最大 2^20) まで上げ、新しい soft を返す (Docker のコンテナの既定は soft 1024)。

### 10.3 継続渡しの API (FEAT-0241)

ライブラリ (CirkaLang の `Std.Async`・`Std.Net`、spec-v0.0.5.md §2) は、待つ操作に一度だけ実行される継続を渡す (ADR-089)。
そのためのランタイムの関数は、ループを引数に取らず、このスレッドで動いているループに働く。

- `cirka_async_run(cb)`: このスレッドに新しいループを作り、`cb` を最初のコールバックとして実行してから `cirka_loop_run` で回し、
  終わったらループを解放する (残った待ちとタイマーのコールバックは呼ばずに解放)。動いているループ (入れ子なら内側) はスレッドの状態に持つ。
- `cirka_async_watch(h, events, ms, cb)`・`cirka_async_timer(ms, cb)`・`cirka_async_cancel(id)`・`cirka_async_stop()`・
  `cirka_async_remote()`・`cirka_async_blocking(work, done)`: 動いているループに対する `cirka_loop_*`。ループが動いていなければ
  プログラムを終了する (`no event loop is running on this thread (run the program in Std.Async.runLoop)`)。
  `watch` は記述子オブジェクト `h` (下記) を取る。
- `cirka_async_events()`: 実行中のコールバックの事象。1 読める、2 書ける、4 エラーか切断 (待っていた事象も立つ)、8 タイムアウト
  (`EV_TIMEOUT`)、待ちを設定できなかったときは `-errno`、タイマーなら 0。
- **待ちのタイムアウト.** `ms` が 0 以上なら待ちにタイマーを付ける (タイマーの要素は fd を持ち、コールバックは fd の待ちの側にある)。
  期限が来ると待ちを終え (fd は登録したままで、その後の事象は覚えるだけ)、コールバックを `EV_TIMEOUT` で呼ぶ。事象が先に来たとき、待ちを置き換えたとき、
  取り消したときは付いているタイマーを取り消す。`cirka_loop_cancel` はふつうのタイマーだけを取り消す。
- **一度だけの保証.** 待ちを設定できないとき (閉じた記述子オブジェクトは `-EBADF`、epoll の失敗) は、コールバックを 0 ミリ秒の
  タイマーにして、次のターンで `-errno` を事象として呼ぶ。継続は、完了・失敗・タイムアウトのどれでもちょうど 1 回実行される。
  例外はループを止めて解放するときで、コールバックは呼ばずに解放する。
- **予算.** コールバックごとに、すぐに完了したソケットの操作 (`cirka_sock_*`) を数え、64 回 (`BUDGET`) を超えるとデータがあっても
  `EAGAIN` を返す (ライブラリは待ちに入り、ほかのコールバックが先に動く)。数はコールバックを始めるたびに 0 に戻し、ループの外では数えない。
  予算で返す `EAGAIN` は `ready` を落とさないので、続く待ちは `MOD` で聞き直し、データが残っていれば次のターンで呼ばれる。
- **準備の状態を使う操作 (FEAT-1001).** `cirka_sock_recv`・`cirka_sock_recvfrom`・`cirka_sock_accept` (読む向き) と `cirka_sock_send`・
  `cirka_sock_sendto` (書く向き) は、動いているループがその記述子オブジェクトの登録について準備ができていないと知っていれば、
  システムコールなしで `EAGAIN` を返す。`EAGAIN` を受けたとき、`cirka_sock_recv` が求めたより少なく読んだとき (ストリーム。
  データグラムは 1 つずつ届くので数えない) にループへ知らせる。HTTP のように 1 つ読んで答える接続は、要求ごとに読み込み・書き込み・
  `epoll_wait` (の一部) だけで済む (1 度だけの待ちでは、空振りの読み込みと再登録の `epoll_ctl` が加わった)。
  空とわかっている受信と予算切れの検査は呼び出し側に展開し、読み込みは別の関数にする (応答を送った後に次の要求を読もうとする受信は
  空とわかっていて、サーバの受信の半分を占める。FEAT-1016)。記述子のスロットは検査で 1 度だけ引き、読み込みの後の記録にも使う。
  このため `EAGAIN` は「ループの知る限りまだない」の意味になる (登録した直後や、最後の `epoll_wait` の後に届いたデータ。続く待ちは次のターンで呼ばれる)。`cirka_sock_recv` はストリーム (TCP) 用で、データグラムは `cirka_sock_recvfrom` で読む (1 つずつ届くので、少ない読み込みは空の印にならない)。記述子オブジェクトを閉じると (`cirka_handle_close`・解放)、動いているループはその登録を忘れる (カーネルも記述子と一緒に登録を消す。通し番号が一周しても古い登録と混ざらない)。
- **記述子オブジェクト.** `cirka_fd_handle(fd)` は記述子を持つオブジェクト (kind `FOREIGN`。語 0 の下位 32 ビットが fd、上位 32 ビットが
  通し番号) を作り、解放されると後始末の関数が記述子を閉じる。通し番号は epoll の登録を見分ける (番号が再利用された fd の古い登録と
  混ざらない)。`cirka_handle_fd(h)` は記述子 (閉じていれば -1)、`cirka_handle_close(h)` は今閉じる (語 0 を閉じた印にするので
  解放のときに閉じ直さない。2 度目は `-EBADF`)。ソケットの操作 `cirka_sock_accept(h)`・`cirka_sock_recv(h, max)` (64 KiB までを
  スレッドごとの受信バッファに `recv` で読み (FEAT-1013。ソケットの `read` はファイルの読み込みとしての検査を通る)、`cirka_fd_read` と同じ規約で返す。ただし受け取ったバイトを返すときは `errno` を書かない (空の結果のときだけ `errno` が意味を持つ: 0 はストリームの終わりか空のデータグラム、`EAGAIN`、エラー。ライブラリは空のときだけ `cirka_errno` を読む。FEAT-1016)。バッファは最初の読み込みで確保し、ランタイムが始めたスレッドは終わりに解放する (C のライブラリのスレッドは、割り当ての空きリストと同じく持ったまま終わる)。
  スタックの 64 KiB の配列はページごとのスタックプローブを読み込みのたびに払った。FEAT-1002)・`cirka_sock_send(h, data, off)` は記述子オブジェクトを取る。
  ライブラリは記述子オブジェクトを線形な値で包むので、待ちの間はソケットを継続が持ち、待っている記述子が閉じられることはない
  (§10.1 の `unwatch` は要らない)。ループと一緒に捨てられた継続が持つソケットも、解放のときに閉じる。
- **UDP.** `cirka_net_udp_bind(host, port)`・`cirka_sock_sendto(h, data, host, port)`・`cirka_sock_recvfrom(h, max)`
  (データグラムの最初の `max` バイト。送り手はスレッドの状態に残し、`cirka_net_peer_host()`・`cirka_net_peer_port()` で読む)。
- **I/O のシステムコール (FEAT-1007).** 記述子の I/O (`read`・`write`・`send`・`sendto`・`recvfrom`・`accept4`・`epoll_wait`・`epoll_ctl`) は
  `syscall` 命令で直接呼ぶ (結果は libc と同じく失敗で -1 と `errno`)。glibc の関数は取り消しの点で、マルチスレッドのプロセス
  (プログラムは専用のスレッドで動くので常にそう。§7) では呼ぶたびに非同期の取り消しを 2 回のアトミックな操作で入れて切る。
  ランタイムはスレッドを取り消さない。ThreadSanitizer のランタイム (`--cfg cirka_tsan`) は libc の関数を呼ぶ: その横取りが、
  記述子を通してデータを渡すスレッドの間の happens-before を作る。

### 10.4 テストと計測

- ランタイムの単体テスト (`evloop.rs`・`net.rs`): タイマーの順序と取り消し、停止と再開、パイプの待ちと切断の知らせ、閉じて再利用された
  記述子、別スレッドからの post、スレッドプール、ループバックのエコー、アドレスとエラー。
- `tests/mir/run/evloop.mir` (`tsan: 1`): タイマー・停止と再開・別スレッドからの post (受け口が生きている間ループが待つ)・
  プールの仕事を、TSan の構成でも実行する。
- `tests/mir/run/echo_10k.mir`: 1 つのスレッドの 1 つのループで、エコーサーバと 1 万のクライアントが 1 万ずつのソケットを同時に開く
  (FEAT-0046 の受け入れ条件)。接続中のクライアントは 256 まで (待ち受けのキューは `somaxconn` = 4096)。最後のクライアントがエコーを
  受け取るとサーバが待ちを取り消してソケットを閉じ、クライアントは終わりを見て閉じる。サーバが先に閉じるので TIME_WAIT はサーバ側の
  4 つ組に残り、続けて実行してもクライアントのエフェメラルポート (既定 28232 個) を使い切らない。
  i5-3320M で 1 回約 1.1 秒 (接続・エコー・切断が 1 万ずつ、確保 110085 回)、最大 RSS 約 3 MB。
- ランタイムの単体テスト (FEAT-0241): タイムアウト付きの待ち、設定できない待ちのコールバック、予算 (64 回で待ちに入り、残りは
  ほかのコールバックの後)、記述子オブジェクトの後始末、データグラムと送り手。
- ランタイムの単体テスト (FEAT-1001): 少なく読んで空にしたソケットの次の読み込みと待ちはシステムコールを使わず (`epoll_ctl` は
  最初の登録の 1 回)、データと終わりを 1 つの事象で知らされても両方を読む。
- CirkaLang の実行テスト `tests/cirkalang/run/v005/` の `async.cirka` (`tsan: 1`)・`async_task.cirka`・`net_tcp.cirka` (1 つのループで 100 の
  クライアント、1 MB の送受信、タイムアウト、エラー、名前解決、`stopLoop` で閉じるソケット)・`net_udp.cirka`・`http_echo.cirka`。
- MIR インタプリタはイベントループとソケットを扱わない (MIR のテストは `skip-backend: interp`)。

## 11. OS 層 (FEAT-0242)

CirkaLang の `Std.Os`・`Std.Process`・`Std.Fs`・`Std.Env` (spec-v0.0.5.md §3) が `extern "cirka"` で呼ぶ関数 (`os.rs`)。
libc の関数を包み、ライブラリが `Result` にしやすい形で返す。

### 11.1 規約

- 失敗は `-errno` で返すか、文字列・オブジェクトを返す関数なら空の値を返して `errno` を設定する (`stdio.rs` と同じ。成功では `errno` を 0 にする)。
- 戻り値 1 つに収まらない結果 (子のパイプ、終了の状態、ファイルの属性、出力の長さ) はスレッドの状態の `os` (6 語) に置き、
  ライブラリはすぐに `cirka_os_result(i)` で読む。次の `os.rs` の呼び出しで上書きされる。
- 文字列とオブジェクトの引数は借用。パスに NUL を含むと `-EINVAL`。

### 11.2 子プロセス

- `cirka_os_spawn(argv, flags)`: `argv` はプログラム名と引数をそれぞれ NUL で終えて並べた文字列。`posix_spawnp` で `PATH` から探して
  起動し、プロセス id か `-errno` (見つからなければ `-ENOENT`) を返す。`flags` は標準入力・出力・エラーごとに 2 ビット
  (0 継承、1 パイプ、2 `/dev/null`) と、エラーを出力へ向ける 64 (`2>&1`)。パイプの親の側は close-on-exec で作り
  (ほかの子へ漏れない)、結果 0〜2 に置く (パイプでなければ -1)。子の側は起動後に閉じる。
- 子は **空のシグナルマスクと既定の `SIGPIPE`** で始める (`POSIX_SPAWN_SETSIGMASK`・`POSIX_SPAWN_SETSIGDEF`)。親がシグナルを
  受け取るためにブロックしていても、`SIGPIPE` を無視していても、シェルから起動したのと同じ状態になる (Rust・Python と同じ)。
- `cirka_os_wait(pid, nohang)`: `waitpid` (EINTR はやり直す)。終わっていれば 0 で、終了コード (シグナルで終われば -1) を結果 0、
  シグナル (なければ 0) を結果 1 に置く。`nohang` でまだ動いていれば 1。`cirka_os_kill(pid, sig)`。
- `cirka_os_communicate(in, out, err, input)`: 子の標準入力へ `input` を書いて閉じながら、出力とエラーを終わりまで読む
  (-1 の記述子は使わない)。記述子を非ブロッキングにして `poll` で回すので、子が入力を読み切る前に多く書いても止まらない。
  書き込みの間は `SIGPIPE` をブロックし、入力を読まずに終わった子への書き込み (`EPIPE`) は残りを捨てるだけにする
  (発生した `SIGPIPE` は、もともとブロックしていなければ `sigtimedwait` で取り除いてからマスクを戻す)。
  出力とエラーをつないだ文字列を返し、出力の長さを結果 0 に置く。記述子はすべて閉じる。

### 11.3 シグナルとスレッドのマスク

- ランタイムはシグナルハンドラを持たない (スタック溢れの報告に使う `SIGSEGV`・`SIGBUS` を除く、§9.1)。プログラムはシグナルを
  **ブロックして待つ** (`cirka_os_signal_wait` = `sigtimedwait`) か、**signalfd で読む** (`cirka_os_signalfd(mask)` はこのスレッドで
  `mask` をブロックし、非ブロッキングで close-on-exec の signalfd を返す。`cirka_os_signalfd_read(h)` は記述子オブジェクトから 1 つ読み、
  なければ `-EAGAIN`。予算 (§10.3) を数える)。非同期のハンドラの中で動かしてよい処理の制約を言語に持ち込まない。
- `cirka_os_signal_action(sig, action)`: 既定 (0) か無視 (1)。`SIGSEGV`・`SIGBUS` は `-EINVAL`。`cirka_os_raise(sig)` (このスレッドへ)、
  `cirka_os_signal_mask(how, mask)` (0 ブロック、1 解除)。マスクはシグナル `s` をビット `s - 1` で表す 1〜63 番 (`SIGSEGV`・`SIGBUS` は除く)。
- **プロセスへ送られたシグナルは、それをブロックしていないスレッドのどれかに届く。** 届いたスレッドに既定の動作 (終了) が働くと、
  プログラムがそのシグナルを signalfd で待っていてもプロセスが終わる。そこでランタイムのスレッドはシグナルを受けない:
  - 最初のスレッド (`main` を動かすスレッドを待つだけ) は、すべてのシグナルをブロックしてから `main` のスレッドを作る。
    `main` のスレッドは起動時のマスクに戻してから始める (§7)。
  - スレッドプール (§10.1) のスレッドは、作る間だけ作る側で非同期のシグナル (障害で起きる `SIGILL`・`SIGTRAP`・`SIGBUS`・`SIGFPE`・
    `SIGSEGV`・`SIGSYS` 以外のすべて) をブロックして作る。新しいスレッドはマスクを継承するので、始まった時点からブロックしている
    (始まってからブロックすると、その前に届いたシグナルで終わる)。
  - 利用者のスレッド (`Std.Thread`) は作ったスレッドのマスクを継承する。シグナルを受け取るプログラムは、スレッドを作る前に
    ブロックする (spec-v0.0.5.md §3.3)。

### 11.4 メモリマップ

- `cirka_os_map_file(path)`: ファイルを読み取り専用 (`MAP_PRIVATE`) で写像し、写像オブジェクト (kind `FOREIGN`。語 0 が `{ptr, len}` の
  状態、後始末の関数が `munmap` する) を返す。空のファイルは空の写像。失敗しても空の写像を返して `errno` を設定する。
- `cirka_os_map_len(m)`・`cirka_os_map_slice(m, off, n)` (写像に切り詰めたコピー)・`cirka_os_map_byte(m, i)` (外なら -1)・
  `cirka_os_map_count(m, byte, from, to)`・`cirka_os_map_find(m, byte, from)` (なければ -1)。範囲は写像に切り詰めるので、
  ライブラリの誤りでも写像の外は読まない。
- ほかのプロセスが写像中のファイルを短くすると、失われた部分を読んだときに `SIGBUS` が起き、ランタイムのハンドラ (§9.1) が
  `segmentation fault` と報告してプログラムを終える (終了コード 139。C のプログラムもシグナルで終わる)。

### 11.5 ディレクトリ・属性・作業ディレクトリ

- `cirka_fs_list(path)`: `.`・`..` を除く名前をそれぞれ NUL で終えて並べた文字列 (ディレクトリの順。並べ替えはライブラリ)。
- `cirka_fs_stat(path, follow)`: `stat` (`follow` が 0 なら `lstat`)。大きさ・種類 (1 ファイル、2 ディレクトリ、3 シンボリックリンク、
  0 そのほか)・許可のビット (`mode & 0o7777`)・更新時刻 (秒) を結果 0〜3 に置く。
- `cirka_fs_mkdir(path, mode)`・`cirka_fs_rmdir(path)`・`cirka_fs_rename(from, to)`・`cirka_os_getcwd()`・`cirka_os_chdir(path)`・
  `cirka_os_hostname()`。

### 11.6 テスト

- ランタイムの単体テスト (`os.rs`): パイプでつないだ子とその終了コード、シグナルで終えた子、ないプログラム、入力を読まない子への
  1 MB の書き込み (止まらず、`SIGPIPE` で終わらない)、シグナルの待ちと signalfd、写像・一覧・属性・改名・削除、作業ディレクトリと
  ホスト名。ヘッダ `cirka_rt.h` がライブラリ用のモジュールの公開する関数をすべて宣言していることも調べる。
- CirkaLang の実行テスト `tests/cirkalang/run/v005/os_tool.cirka` (spec-v0.0.5.md §3.6) は、プロセスへ送ったシグナルをループで受ける前に
  スレッドプールを起動しておく。最初のスレッドかプールのスレッドがシグナルをブロックしていなければ、`SIGUSR2` でプロセスが終わる。
