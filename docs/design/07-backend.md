# 07. LLVM バックエンド

## 1. 方針

- MIR から **LLVM IR をテキスト (`.ll`) で生成** し、`clang` (`$CIRKA_CLANG`) で最適化・コンパイル・リンクする (ADR-009)。
- 対象 LLVM は 23 (apt.llvm.org の Ubuntu 24.04 向けパッケージ)。比較の相手の Rust (rust-toolchain.toml の rustc) が使う LLVM と同じメジャー版にし、
  Rust の版を上げたら合わせて上げる (ADR-110、FEAT-1000)。版は `CIRKA_LLVM_VERSION` で選び、Docker イメージは LLVM を最後の層で入れる。
  生成する IR は LLVM 15 以降の **opaque pointer** 形式のみを使い、バージョン依存の構文を避ける (同じ IR を LLVM 18 と 23 がそのまま読む)。
- ターゲット: `x86_64-unknown-linux-gnu`。

## 2. 変換規則

| MIR | LLVM IR |
|-----|---------|
| 関数 | `define tailcc <ret> @<mangled>(...)` |
| `tailcall` | `musttail call tailcc ...` + `ret` (末尾呼び出し保証) |
| 自己末尾呼び出し (`tailcall f` を `f` の中で) | 引数を phi に持つループの先頭 `tailrec` への `br` (入口 `entry` → `tailrec` → `bb0`、引数は `%vN.in`)。LLVM のループ最適化が効く (FEAT-0066)。呼び出しが 2 つ以上なら、どれも 1 つのラッチ `tailrec.latch` (引数を phi で合わせて `tailrec` へ) に飛ぶ (FEAT-1005): C や Rust のループの本体の終わりと同じ形。呼び出しごとに後ろ向きの辺があると、LLVM のループの正規化は引数を変えずに渡す呼び出しを内側のループとして切り出し、1 反復に分岐が 2 回成立する配置になる (bench/array-sort の分割のループ) |
| `call` (非末尾) | `call tailcc` |
| 複数の結果を返す関数 (`-> (ptr, i64)`、FEAT-0065) | 構造体を返す `define tailcc { ptr, i64 }`。`ret (a, b)` は `insertvalue`、`let (a, b) = call` は `call` と `extractvalue` |
| `extern f(...)` | `call ccc` (C ABI。構造体の値渡しは `clang` の ABI 規則に従って展開)。C シグネチャ付きなら引数を C の型へ (`trunc`/`inttoptr`/`fptrunc`)、結果を `i64`/`double` へ (`sext`/`zext`/`ptrtoint`/`fpext`) 変換する |
| `dup x` | インライン: `rc = load i32`、`rc == 0 → 何もしない` (不死の静的なオブジェクト)、`rc > 0 → store rc+1`、`rc < 0 → call @cirka_dup` (共有。冷たいブロック)。RC 0 は速い経路の前に調べ、同じ線の中で前へ飛ぶだけにする (FEAT-1017。速い経路の後に調べると、引数やランタイムの結果として届く静的なオブジェクト (空文字列・IO トークン・`noTimeout` のような静的な構築子) は関数の末尾の冷たいブロックへ飛んで戻り、使わないキャッシュラインに触れる) |
| `drop x` | インライン: `rc == 0 → 何もしない` (同上)、`rc > 1 → store rc-1`、`rc == 1 → call @cirka_free_object`、`rc < 0 → call @cirka_dec_ref_slow` (後の 2 つは冷たいブロック) |
| `dup y₁..yₖ; drop x` / `dup y₁..yₖ; let t = drop_reuse x` (yᵢ は x のフィールドの射影) | **drop 特殊化**: `rc(x) == 1` なら dup と対応するフィールドの drop を打ち消し、残りのポインタフィールドだけ drop して `x` のセルを解放 (または再利用トークンにする)。そうでなければ yᵢ を dup して x を drop。セルの解放 (と一意な環境のクロージャの呼び出しでの環境の解放) は大きさがわかるので、スレッドの状態の `free_fast` が 1 ならそのクラスのフリーリストに積んで `frees` と `max_live` を更新し、0 なら `call @cirka_free_raw` (FEAT-1027、05-runtime.md §3) |
| `alloc` | インライン (FEAT-1027): 256 バイト以下なら、スレッドの状態 (`@cirka_tls_block`、local-exec の TLS) のそのクラスのフリーリストから先頭を外し、`allocs` を 1 つ増やす。リストが空なら `call @cirka_alloc` (起こりにくい重み)。大きなオブジェクトと検査付きのランタイム (`debug_rt`) は `call @cirka_alloc`。ヘッダを store |
| `switch` | `switch i16 tag` (ヘッダの tag を load)。既定の腕がなければ既定の行き先は `panic`。ただし腕が型のすべての構成子を覆うとき (Core → MIR は網羅的でない照合に明示の `panic` の腕を付ける) は最後の腕を既定の行き先にし、腕が 1 つなら tag を読まず無条件に分岐する。検査付きのランタイム (`debug_rt`) では `panic` を残す (FEAT-0093) |
| `extern cirka_errno` | インライン (FEAT-1027): スレッドの状態に `errno` のアドレス (FEAT-1021) があればそこから読み、なければ `call @cirka_errno` |
| `force` | `kind == THUNK/IND` の判定をインライン、評価が必要な場合のみ `call @cirka_force` |
| `global g = f` | CAF: `@"g.g" = internal global { i64, ptr }` (サンクのヘッダと `f` を呼ぶスタブ)。`@cirka_caf_roots` に並べる。静的な大域変数 (FEAT-1006): `@"g.g" = internal constant { i64, … }` (構築子のヘッダ (RC 0) と、スロットの順のフィールド: 整数・`double`・文字列や大域変数や引数のない構築子の静的なオブジェクトへの `ptr`)。CAF の根にもスタブにも入れない。文字列リテラル (`@str.N = private constant { i64, i64, [n x i8] }`: ヘッダ・長さ・バイト) と引数のない構築子 (`@con.T.C = private constant i64`: ヘッダだけ) も定数で、LLVM は読み出し (リテラルの長さ・探す文字列のバイト・構築子のタグ) を畳み込める。CAF は評価で書き換えるので `global` のまま (FEAT-1020) |
| `extern cirka_bytes_copy` / `cirka_bytes_eq` / `cirka_bytes_index` (バイト列のコピー・範囲の比較・バイトの探索、FEAT-0095) | 呼び出さずにインラインで生成する (FEAT-0097): 長さ 0 以下の判定、範囲の検査 (失敗は `panic`)、`llvm.memmove` / `memcmp(..) == 0` / バイトの探索。ランタイムの関数と同じ意味で、長さが定数なら LLVM がロード・ストアと比較に展開する。バイトの探索 (FEAT-1019) は 16 バイトのベクトルの比較 (`<16 x i8>` の `icmp eq` を `i16` のマスクにして `cttz`) で範囲の先頭から 64 バイトまでを調べ、16 バイトに満たない端は範囲の終わりで終わる 16 バイトを読み直す。残りが 64 バイトを超えるときだけ `memchr` を呼ぶ。16 バイトずつの比較は展開しないループのまま残し (`llvm.loop.unroll.disable`)、16 バイトより短い範囲の経路と `memchr` の経路には起こりにくい重みを付けて、比較の経路の外に置く (FEAT-1025: 展開した 4 つの比較とその出口が並ぶと、探索の熱い経路が多くのキャッシュラインに散らばる)。範囲が 16 バイトより短いときは文字列の中の 16 バイトの窓を読んで範囲の外のレーンをマスクで捨て、文字列が 16 バイトより短いときはバイトのループで調べる。読むのはどれも文字列の `[0, len)` の中だけで、範囲の外のバイトは結果に影響しない |

- 属性・メタデータ (FEAT-0058、16-performance.md §3): 生成するすべての関数に `nounwind` (panic=abort の間)。オブジェクトのヘッダ (RC・タグ) と
  フィールドの load/store に別々の `!tbaa` を付ける (生きているオブジェクトの中で重ならない)。`cirka_alloc` の結果は `noalias nonnull`、`cirka_panic` は `cold`。 MIR の `cold fn` (FEAT-0070、一意な配列のループの元の本体) は `cold noinline`。 プログラムは 1 つの LLVM モジュールで、外から呼ばれるのは `cirka_main` だけなので、それ以外の関数はすべて `internal` にする (FEAT-0072。LLVM が呼び出しの定数を伝え、1 か所からの関数を丸ごとインライン化できる)。
- LLVM の末尾再帰の除去 (TailCallElim。LLVM 18 で確認) は、自分自身の呼び出しの結果に足す関数を累積引数のループにするとき、別の関数への `musttail` の後の `ret` も書き換えて
  モジュールを壊す。そのような関数 (自分自身の非末尾呼び出しと、他の関数への `musttail` を両方含むもの) には `"disable-tail-calls"="true"` を付けて除外する
  (`musttail` は保証されたまま。FEAT-0065: unboxed の結果でこの形が増えた)。
- 名前マングリング: `_C<モジュール長><モジュール名><名前長><名前>`(Rust v0 風。`achc` / `cirka` の両方で `--demangle` を用意)。
- 呼び出し規約 `tailcc` はすべて x86_64 で `musttail` を保証する。aCHC / CirkaLang から生成した関数どうしの呼び出しはすべて `tailcc`。
- アンワインド導入後 (FEAT-0040) は、drop が必要な値を持つ呼び出しを `invoke` にし、landing pad で drop して `resume`。

## 3. 最適化レベル

| フラグ | Core/MIR 側 (共通) | LLVM 側 |
|--------|---------|---------|
| `-O0` | Core 最適化なし、Perceus の最小限のみ | `-O0` |
| `-O1` | 簡約・インライン・借用推論 (aCHC は正格性解析も) | `-O1` |
| `-O2` (release 既定) | 全パス (特殊化・worker/wrapper・再利用解析・エスケープ解析) | `-O3` (Rust の release の `opt-level=3` と同じ強さで比べる。FEAT-0072) |

- `-O2` では、浮動小数点の **順序を保つ縮約** のベクトル化を有効にする (`-mllvm -force-ordered-reductions`、FEAT-1010、ADR-115)。
  和を累積するループの加算はレーンの順に 1 つずつ行い、ほかの演算 (割り算・掛け算・整数からの変換) を SIMD でまとめる。
  加算の順序は元のループと同じなので結果は変わらない (浮動小数点の加算は結合則が成り立たないので、順序を変える縮約は使わない)。
  x86-64 では LLVM の既定で無効 (Rust も無効)。ベクトル化するかは LLVM の費用のモデルが決める。

## 4. リンク

```
$CIRKA_CLANG -O3 --target=$CIRKA_TARGET -Wno-override-module out.ll -lm <runtime>/libcirka_rt.a -lm -lpthread -Wl,--gc-sections -o a.out
```

(最適化の引数は §3。リンカはシステムの既定の GNU ld。)

- `libcirka_rt.a` は `cargo build -p cirka-runtime` で作り、`achc` 実行ファイルからの相対パス、または `CIRKA_RUNTIME_DIR` で探す。
- `-lm` はランタイムより前にも置く。`libcirka_rt.a` には Rust の compiler-builtins の数学関数 (`cbrt`・`fma`・`floor` など) が
  弱いシンボルとして入っており、後ろに置くとプログラムの呼び出しがそちらに解決されて C ライブラリの関数と結果や性能が変わる (BUG-0028)。
- FFI で指定されたライブラリ (`extern` の `link` 属性、`achc.toml` の `[link]`) を追加する。
- コードの配置 (FEAT-1023): プログラムの演算ごとに動くランタイムの関数 (確保と解放・RC の遅い経路・`force`・部分適用・イベントループとそれが動かすソケットの I/O・`errno`) は `.text.hot.<名前>` の節に置き、生成コードのクロージャの呼び出し (`cirka_apply_N`・`cirka_call_1`) には `hot` 属性を付ける (LLVM は `.text.hot.` の節に置く)。GNU ld の既定のリンカスクリプトは `.text.hot.*` をテキストの先頭にまとめるので、プログラムが実行するランタイムのコードは数ページに収まる (ランタイムの 40 KiB ほどのコードに散らばると、システムコールの後に冷えた TLB とキャッシュをページごとに引き直す)。初期化・遅い経路・エラーの処理は既定の節のまま。
- ランタイムの作り方 (FEAT-1026、ADR-120): `libcirka_rt.a` は実行ファイルの位置独立コード (`-C relocation-model=pie`) として、
  1 つのコード生成単位 (`[profile.release.package.cirka-runtime] codegen-units = 1`) で作り、staticlib だけを出力する。ランタイム
  自身の関数と静的な変数は PC 相対で直接参照される。rustc の既定 (共有ライブラリ向けの PIC、複数の単位、rlib も出力) では、
  別の単位の関数の呼び出しと外から見える静的な変数の読み出しが GOT の項を通り (再配置できない `GOTPCREL` なのでリンカも直さない)、
  GOT は専用のページにある。`scripts/build-runtime.sh` とドライバ (`native.rs`) は同じ `RUSTFLAGS` で作る (違うと cargo が
  たがいに作り直す)。生成コードの関数 (`cirka_call_1`) はランタイムからは外部の名前なので GOT を通る: イベントループは
  そのアドレスをスレッドの状態に一度だけ置いて呼ぶ。
- 将来: LTO で `libcirka_rt` の高速経路も含めてインライン化(ランタイムを LLVM bitcode でも出力)。

## 5. デバッグ情報

第一段階後半で DWARF の行情報を出力する (`-g`)。MIR の各命令は Core/ソースの `Span` を保持しているため、それを `!dbg` に変換する。
