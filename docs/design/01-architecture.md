# 01. コンパイラアーキテクチャ

二つの試作 (aCHC と CirkaLang) は **フロントエンドだけが異なり、消去後の共通 Core 以降をすべて共有** する (ADR-020)。

## 1. パイプライン

```
┌─ aCHC フロントエンド (achc-*) ───────────────────────────────────────────────┐
│ .hs / .chs                                                                   │
│   │ achc-syntax    字句解析 → レイアウト規則 (Haskell 2010 §10.3) → 構文解析 │
│   │ achc-rename    モジュール・import/export・名前解決・fixity の再結合       │
│   │ achc-typeck    HM + 型クラス + 多重度 + 種 Type/LType、一意性・借用・Drop │
│   │                → 辞書渡しへの elaboration                                │
│   │ achc-desugar   do / ガード / where / パターンマッチの決定木 → 共通 Core   │
└───┼──────────────────────────────────────────────────────────────────────────┘
    │
┌─ CirkaLang フロントエンド (cirkalang-*) ─────────────────────────────────────┐
│ .cirka                                                                       │
│   │ cirkalang-syntax  字句解析 → レイアウト → 構文解析                        │
│   │ cirkalang-resolve モジュール・名前解決                                    │
│   │ cirkalang-elab    双方向型検査 (QTT 依存型・宇宙 Type/LType・トレイト)    │
│   │                   → TT (依存型付きコア、TT Lint で検証)                   │
│   │ cirkalang-check   全域性・終了性検査、部分集合型の証明義務 → cirka-smt    │
│   │ cirkalang-lower   多重度 0 の消去・依存型の Any/cast 化 → 共通 Core       │
└───┼──────────────────────────────────────────────────────────────────────────┘
    ▼
┌─ 共通基盤 (cirka-*) ─────────────────────────────────────────────────────────┐
│ 共通 Core (System F + 多重度 + ADT + Any/cast)       ← Core Lint で常に検証    │
│   │ cirka-core   簡約・インライン・特殊化・worker/wrapper・unboxing・case-of-case │
│   │              (aCHC のみ: 正格性解析)                                       │
│   │ cirka-mir    Core→MIR: A 正規形化・クロージャ変換・サンク・表現の決定・    │
│   │              自動 Drop/Release → Perceus (dup/drop・借用推論・再利用)       │
│ MIR (所有権・RC 明示)                                ← MIR 所有権検証器        │
│   │ cirka-codegen-llvm  MIR → LLVM IR (テキスト)                               │
│   ▼                                                                           │
│ .ll ─clang─► .o ─clang/lld + libcirka_rt.a + libc + libm─► 実行ファイル        │
└──────────────────────────────────────────────────────────────────────────────┘
```

MIR には **インタプリタ** (`cirka-mir::interp`) を用意し、コード生成の参照実装・テストのオラクルとする。

## 2. 参考にした実装との対応

| 参考 | aCHC | CirkaLang | 備考 |
|------|------|-----------|------|
| GHC Parser / Renamer | `achc-syntax` / `achc-rename` | `cirkalang-syntax` / `cirkalang-resolve` | 手書き再帰下降 + Pratt |
| GHC Typechecker | `achc-typeck` (HM + 型クラス) | — | |
| Idris 2 の elaborator / QTT | — | `cirkalang-elab` (TT) | 双方向型検査・NbE・単一化 |
| Idris 2 / Lean 4 の全域性検査 | — | `cirkalang-check` | |
| GHC Core (System FC) | 共通 Core | 共通 Core | `Any`/`cast` で依存型の消去結果を表す |
| GHC Simplifier 等 | `cirka-core::opt` | 同左 | |
| GHC STG + Cmm | **MIR** | 同左 | 所有権・RC・サンク・クロージャを明示 |
| Koka / Lean 4 の Perceus・RC 最適化 | `cirka-mir::perceus` | 同左 | |
| GHC RTS (GC あり) | `cirka-runtime` (**GC なし**) | 同左 | |

## 3. クレート構成 (Cargo ワークスペース)

| クレート | 種別 | 役割 | 主な依存 |
|----------|------|------|----------|
| **共通基盤** | | | |
| `cirka-diagnostics` | lib | ソース位置 (`Span`, `FileId`)、診断の構築と表示 | |
| `cirka-core` | lib | 共通 Core・Core Lint・プリティプリンタ・最適化パス | diagnostics |
| `cirka-mir` | lib | MIR・テキスト形式・Core→MIR・Perceus・検証器・インタプリタ | core |
| `cirka-codegen-llvm` | lib | MIR → LLVM IR テキスト | mir |
| `cirka-runtime` | staticlib (`#![no_std]`) | オブジェクト表現・RC・アロケータ・サンク・I/O・スレッド・起動 | `libc` のみ |
| `cirka-bigint` | lib (`#![no_std]`) | 多倍長整数の演算 (加減乗除・10 進変換・`f64` との正確な変換)。確保をせず呼び出し側のバッファに書く。ランタイムと MIR インタプリタが共有 | なし |
| `cirka-smt` | lib | SMT-LIB2 生成・Z3 呼び出し・キャッシュ・反例 | diagnostics |
| `cirka-bindgen` | lib + bin | libclang による C ヘッダ取り込み (出力は aCHC / CirkaLang 両方) | |
| `cirka-driver` | lib | Core 以降のパイプライン実行・外部ツール起動・`_build/`・`.env` 設定 | core, mir, codegen |
| **aCHC** | | | |
| `achc-syntax` | lib | Haskell のトークン・レイアウト・AST・パーサ | diagnostics |
| `achc-rename` | lib | モジュールグラフ・スコープ・fixity | achc-syntax |
| `achc-typeck` | lib | 型・種・多重度・型クラス・一意性・借用 | achc-rename, core |
| `achc-desugar` | lib | Typed AST → 共通 Core | achc-typeck, core |
| `achc` | bin | CLI (`achc.toml`) | 上記 + driver |
| **CirkaLang** | | | |
| `cirkalang-syntax` | lib | トークン・レイアウト・AST・パーサ | diagnostics |
| `cirkalang-resolve` | lib | モジュール・名前解決 | cirkalang-syntax |
| `cirkalang-elab` | lib | TT の定義・正規化・双方向型検査・単一化・TT Lint | cirkalang-resolve |
| `cirkalang-check` | lib | 全域性・終了性・部分集合型の証明義務 (SMT) | cirkalang-elab, smt |
| `cirkalang-lower` | lib | 消去と TT → 共通 Core | cirkalang-elab, core |
| `cirka` | bin | CLI (`cirka.toml`) | 上記 + driver |

**依存方向の規則**: 共通基盤のクレートはフロントエンド (`achc-*` / `cirkalang-*`) に依存しない。
aCHC と CirkaLang のフロントエンドは互いに依存しない。

## 4. 主要データ構造の方針

- **アリーナ + インデックス**: AST/IR ノードは `Vec` に格納し `u32` ID で参照。
- **名前**: `Name { id: u32, text: Symbol }`。文字列はインターン化。
- **Span** はすべてのノードに付け、TT・Core・MIR にも保持する。
- **パスは純関数的に**: 各パスは入力 IR を受け取り新しい IR を返す。検証器 (TT Lint / Core Lint / MIR 検証器) をデバッグビルドでは各パス後に実行。
- **ダンプ**: `--dump=<段階>` で各段階を出力 (aCHC: `rn,tc`、CirkaLang: `ast,rn,tt,erased`、共通: `core,core-opt,mir,mir-opt,llvm`)。スナップショットテストに使う。

## 5. CLI

### aCHC

```
achc build [--release] [-o out] [Main.hs]   # achc.toml があればそれに従う
achc run Main.hs [-- args...]
achc check Main.hs                          # 型・所有権検査のみ
achc verify Main.hs                         # (後期 H6) 洗練型・契約の検証 (FEAT-0151, 0152)
```

フラグ: `-O0|-O1|-O2`、`--ownership=off|warn|strict` (`achc.toml` より優先)、`--cycles=warn|error|collect`、`--runtime=release|debug`、`--emit=llvm|obj|exe`。

```toml
# achc.toml (tsconfig に相当)
[package]
name = "hello"
main = "src/Main.hs"
source-dirs = ["src"]

[dependencies]
cirka-base = { path = "../lib/cirka-base" }

[compiler]
ownership = "warn"        # off | warn | strict : .hs に対する所有権関連の診断レベル (02 §1.3)
cycles = "warn"           # warn | error | collect
default-extensions = []
```

### CirkaLang

```
cirka build [--release] [-o out]            # cirka.toml に従う
cirka run [file.cirka] [-- args...]
cirka check                                 # 型検査・所有権・全域性・証明 (コード生成しない。ソース内のテストも検査)
cirka test [path] [--filter s]              # ソース内のテストをテストごとに別プロセスで実行 (12-cirkalang.md §11)
cirka bindgen header.h -o foo.cirka         # C ヘッダから extern 宣言を生成
cirka --version                             # 例: CirkaLang v0.0.1
```

フラグ: `-O0|-O1|-O2`、`--runtime=release|debug`、`--contracts=off|check|verify`、`--emit=llvm|obj|exe`。

```toml
# cirka.toml
[package]
name = "hello"
version = "0.1.0"
edition = "v0.0.1"         # 対象とする CirkaLang の版 (プロトタイプ間の非互換を検出するため)

[dependencies]
core = { path = "../lib/cirkalang/core" }
std  = { path = "../lib/cirkalang/std" }

[verify]
smt = true                 # 部分集合型の証明義務を SMT で解く (false なら手動証明のみ)
contracts = "verify"       # off | check | verify
```

外部ツールのパス (`clang`、`z3` 等) は設定ファイルではなく環境変数 (`.env`) から読む。

## 6. 中間生成物

`_build/<入力ファイル名>/` (`--build-dir=` で変更可) 以下に `.ll`、`.o`、ダンプ (`<名前>.core` / `.opt.core` / `.mir` / `.opt.mir` / `.ll`)、`run` 用の実行ファイルを出力する。SMT のキャッシュは `_build/verify/`。
共通部分 (Core Lint 以降・CLI の引数処理) は `cirka-driver` の `pipeline` / `cli` にあり、両 CLI はフロントエンド (ソース → Core) だけを差し替える。`.core` ファイルはどちらの CLI でも直接入力できる (FEAT-0032)。
全体プログラムコンパイルのため、モジュール単位のキャッシュ (型検査結果) は性能が問題になってから追加する。

## 7. 版の表示

- `cirka --version` は `CirkaLang v<版>` を表示する。版はワークスペースの `cirkalang-*` クレートと `cirka` の `version` に一致させ、
  リリースチケットで更新する。開発中は `v0.0.N-dev`。
- `achc --version` は `aCHC <コミットハッシュ>` を表示する (aCHC には版を付けない: ADR-024)。
