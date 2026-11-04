# 01. コンパイラアーキテクチャ

## 1. パイプライン

```
ソース (.hs / .chs)
  │ cirka-syntax     字句解析 → レイアウト規則 (Haskell 2010 §10.3) → 構文解析
  ▼
AST (ソース位置付き, 演算子は未解決の列)
  │ cirka-rename     モジュール解決・import/export・名前解決・演算子優先度の再結合 (fixity)
  ▼
Renamed AST (一意な Name)
  │ cirka-typeck     依存解析(SCC) → 型推論 (HM + 型クラス + 多重度 + 種 Type/LType)
  │                  → 一意性・借用・Drop 検査 → 辞書渡しへの elaboration
  ▼
Typed AST (型・辞書・多重度注釈付き)
  │ cirka-desugar    do / リスト内包 / ガード / where / パターンマッチのコンパイル (決定木)
  ▼
Core (明示的に型付けされた System F + 多重度 + ADT)       ← Core Lint で常に検証
  │ (opt-in) cirka-verify  洗練型・契約・終了性の検証条件を生成し Z3 で検証 (13-verification.md)
  │                         証明できた事実を Core に注釈 (境界検査の除去などに使う)
  │ cirka-core       Core→Core 最適化: 簡約・インライン・型クラス特殊化・
  │                  正格性解析・worker/wrapper・unboxing・case-of-case・デッドコード除去
  ▼
Core (最適化済)
  │ cirka-mir        Core→MIR: A正規形化・クロージャ変換・ラムダ持ち上げ・サンクの明示化・
  │                  データ表現の決定 (boxed/unboxed)・自動 Drop/Release 挿入
  │                  → Perceus: dup/drop 挿入 → 借用推論 → drop 特殊化 → 再利用解析
  ▼
MIR (所有権・RC操作が明示された一階の命令列)            ← MIR 所有権検証器で常に検証
  │ cirka-codegen-llvm   MIR → LLVM IR (テキスト)
  ▼
.ll ──clang (opt -O2 / llc)──► .o ──clang/lld + libcirka_rt.a + libc + libm──► 実行ファイル
```

MIR には **インタプリタ** (`cirka-mir::interp`) を用意し、コード生成の参照実装・テストのオラクルとする。

## 2. GHC との対応

| GHC | aCHC | 備考 |
|-----|------|------|
| Parser (happy/alex) | `cirka-syntax` (手書き再帰下降 + Pratt) | エラー回復とソース位置を重視 |
| Renamer | `cirka-rename` | |
| Typechecker (OutsideIn(X)) | `cirka-typeck` | 第一段階は HM + 型クラス + RankN(注釈必須) から開始。GADTs/型族は後で |
| Desugarer | `cirka-desugar` | |
| Core (System FC) | Core (System F + 多重度) | coercion は `newtype` 用に最小限のみ (`cast`) |
| Simplifier / Specialise / DmdAnal / WorkWrap | `cirka-core::opt` | |
| STG + Cmm | **MIR** | 所有権・RC・サンク・クロージャを明示 |
| NCG / LLVM backend | `cirka-codegen-llvm` | LLVM のみ |
| RTS (C, GC あり) | `cirka-runtime` (Rust, **GCなし**) | |

## 3. クレート構成 (Cargo ワークスペース)

| クレート | 種別 | 役割 | 主な依存 |
|----------|------|------|----------|
| `cirka-diagnostics` | lib | ソース位置 (`Span`, `FileId`)、診断の構築と表示 | `ariadne` 等(表示) |
| `cirka-syntax` | lib | トークン・レイアウト・AST・パーサ | diagnostics |
| `cirka-rename` | lib | モジュールグラフ、スコープ、`Name` 割当、fixity | syntax |
| `cirka-typeck` | lib | 型・種・多重度・型クラス・一意性・借用の検査と推論 | rename |
| `cirka-core` | lib | Core IR 定義・Lint・プリティプリンタ・最適化パス | |
| `cirka-desugar` | lib | Typed AST → Core | typeck, core |
| `cirka-mir` | lib | MIR 定義・テキスト形式パーサ・Core→MIR・Perceus・検証器・インタプリタ | core |
| `cirka-verify` | lib | 洗練型の解析・Liquid 推論・VC 生成・SMT-LIB2 出力・Z3 呼び出しとキャッシュ・終了性検査 | core, typeck(注釈の型) |
| `cirka-codegen-llvm` | lib | MIR → LLVM IR テキスト | mir |
| `cirka-runtime` | staticlib (`#![no_std]`) | オブジェクト表現・RC・アロケータ・サンク評価・I/O・起動 | `libc` のみ |
| `cirka-driver` | lib | パッケージ/モジュール探索・`achc.toml`・パイプライン実行・外部ツール起動 | 全部 |
| `achc` | bin | CLI | driver |

依存方向は上から下への一方向のみ。IR 定義クレートは前段のクレートに依存しない(Core は typeck に依存しない)。

## 4. 主要データ構造の方針

- **アリーナ + インデックス**: AST/IR ノードは `Vec` に格納し `u32` ID で参照(`la-arena` 風)。所有権問題と性能の両面で有利。
- **名前**: `Name { id: u32, text: Symbol }`。文字列はインターン化 (`Symbol(u32)`)。
- **Span** はすべてのノードに付け、Core/MIR にも保持する(エラー・デバッグ情報用)。
- **パスは純関数的に**: 各パスは入力 IR を受け取り新しい IR を返す。検証器 (`lint`) をデバッグビルドでは各パス後に実行。
- **ダンプ**: `achc --dump=ast,rn,tc,core,core-opt,mir,mir-opt,llvm` で各段階を出力。スナップショットテストに使う。

## 5. ドライバと CLI

```
achc build [--release] [-o out] [Main.hs]   # achc.toml があればそれに従う
achc run Main.hs [-- args...]               # ビルドして実行
achc check Main.hs                          # 型・所有権検査のみ (コード生成しない)
achc bindgen header.h -o Foo.chs            # C ヘッダから extern 宣言を生成 (FEAT-0061)
achc verify [--report] Main.hs              # 洗練型・契約・終了性を検証 (FEAT-0090〜)
achc --dump=<stages> ...                    # IR ダンプ
achc --version
```

主要フラグ: `-O0|-O1|-O2`、`--ownership=off|warn|strict`、`--cycles=warn|error|collect`、
`--runtime=release|debug`(debug ランタイムはリークチェック・RC整合性チェック付き)、`--emit=llvm|obj|exe`、
`--verify`(ビルド時に検証も行う)、`--contracts=off|check|verify`。

### `achc.toml`(プロジェクト設定; tsconfig に相当)

```toml
[package]
name = "hello"
version = "0.1.0"
main = "src/Main.hs"
source-dirs = ["src"]

[dependencies]            # ソースからビルドするパッケージ
cirka-base = { path = "../lib/cirka-base" }

[compiler]
ownership = "warn"        # off | warn | strict : .hs に対する所有権関連の診断レベル
cycles = "warn"           # warn | error | collect
default-extensions = []   # 例: ["Strict", "LinearTypes"]

[verify]                  # 検証機能 (opt-in)
enabled = false           # true でビルド時にも検証
contracts = "off"         # off | check | verify
level = "error"           # 検証失敗を warn / error のどちらにするか
```

外部ツールのパス(`clang` 等)は `achc.toml` ではなく環境変数(`.env`)から読む。

## 6. 中間生成物

`_build/<profile>/` 以下に `.ll`、`.o`、ダンプを出力する。第一段階は全体プログラムコンパイルのため、
モジュール単位のキャッシュ(パース・型検査結果の `.chi`)は性能が問題になってから追加する(FEAT-0078)。
