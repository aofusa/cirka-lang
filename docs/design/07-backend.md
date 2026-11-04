# 07. LLVM バックエンド

## 1. 方針

- MIR から **LLVM IR をテキスト (`.ll`) で生成** し、`clang` (`$CIRKA_CLANG`) で最適化・コンパイル・リンクする (ADR-009)。
- 対象 LLVM は 18(Ubuntu 24.04 の apt パッケージ)。生成する IR は LLVM 15 以降の **opaque pointer** 形式のみを使い、
  バージョン依存の構文を避ける。
- ターゲット: `x86_64-unknown-linux-gnu`。

## 2. 変換規則

| MIR | LLVM IR |
|-----|---------|
| 関数 | `define tailcc <ret> @<mangled>(...)` |
| `tailcall` | `musttail call tailcc ...` + `ret` (末尾呼び出し保証) |
| `call` (非末尾) | `call tailcc` |
| `extern f(...)` | `call ccc` (C ABI。構造体の値渡しは `clang` の ABI 規則に従って展開) |
| `dup x` | インライン: `rc = load i32; if rc > 0 { store rc+1 } else if rc < 0 { atomicrmw sub } ` (0 は不死で何もしない) |
| `drop x` | インライン: `rc > 1 → store rc-1`、`rc == 1 → call @cirka_free_object`、`rc < 0 → call @cirka_dec_ref_slow` |
| `alloc` | インライン: スレッドローカルなフリーリストから pop、空なら `call @cirka_alloc`。ヘッダを store |
| `switch` | `switch i16 tag` (ヘッダの tag を load) |
| `force` | `kind == THUNK/IND` の判定をインライン、評価が必要な場合のみ `call @cirka_force` |

- 名前マングリング: `_C<モジュール長><モジュール名><名前長><名前>`(Rust v0 風、`achc --demangle` を用意)。
- 呼び出し規約 `tailcc` はすべて x86_64 で `musttail` を保証する。Haskell 関数どうしの呼び出しはすべて `tailcc`。
- 例外導入後 (FEAT-0070) は、drop が必要な値を持つ呼び出しを `invoke` にし、landing pad で drop して `resume`。

## 3. 最適化レベル

| フラグ | aCHC 側 | LLVM 側 |
|--------|---------|---------|
| `-O0` | Core 最適化なし、Perceus の最小限のみ | `-O0` |
| `-O1` | 簡約・インライン・正格性解析・借用推論 | `-O1` |
| `-O2` (release 既定) | 全パス (特殊化・worker/wrapper・再利用解析・エスケープ解析) | `-O2` |

## 4. リンク

```
$CIRKA_CLANG -O2 --target=$CIRKA_TARGET -c out.ll -o out.o
$CIRKA_CLANG -fuse-ld=lld out.o <prefix>/lib/libcirka_rt.a -lm -lpthread -o a.out
```

- `libcirka_rt.a` は `cargo build -p cirka-runtime` で作り、`achc` 実行ファイルからの相対パス、または `CIRKA_RUNTIME_DIR` で探す。
- FFI で指定されたライブラリ (`extern` の `link` 属性、`achc.toml` の `[link]`) を追加する。
- 将来: LTO で `libcirka_rt` の高速経路も含めてインライン化(ランタイムを LLVM bitcode でも出力)。

## 5. デバッグ情報

第一段階後半で DWARF の行情報を出力する (`-g`)。MIR の各命令は Core/ソースの `Span` を保持しているため、それを `!dbg` に変換する。
