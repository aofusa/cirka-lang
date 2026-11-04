# AGENTS.md — CirkaLang / aCHC 開発ガイド

このファイルは、本リポジトリで作業するAIエージェントおよび人間の開発者向けの作業規約です。
作業を始める前に必ず全体を読んでください。

## 1. プロジェクト概要

**CirkaLang** は、**バグがなく仕様を満たすことを証明でき (Idris 2 / Clean / Lean 4 のような証明系)、Haskell のような表現力を持ち、
Rust のように GC なしでメモリ安全かつ高速で、CUI・サーバ・GUI (Vulkan)・syscall などの通常のプログラミングができる言語** を目指すプロジェクトです。

優先順位: **(1) 性能と GC なしのメモリ安全** → **(2) 証明系** → **(3) 実用性の拡大**。

### 段階と試作

| 段階 | 成果物 | 内容 |
|------|--------|------|
| 第一段階 | **aCHC** (aofusa's CirkaHaskellCompiler) — 試作A | Haskell の拡張としての試作 (スコープ縮小版)。既存 Haskell (`.hs`) を GC なしで実行し、`.chs` で所有権拡張を試す。TypeScript と JavaScript の関係を目指す。リリース対象外 (版なし) |
| 第一段階 | **CirkaLang v0.0.x** — 試作B | 最終目標の言語のプロトタイプ。正格・QTT 依存型・所有権・証明。**v0.0.1 から始め、プロトタイプごとに v0.0.2, v0.0.3, … と版を上げる** |
| 第二段階 | **CirkaLang v0.1.0 〜 v1.0.0** | 第一段階の評価をもとに正式な言語として作り込む。仕様が完全に固定されたら v1.0.0 |

二つの試作は **共通基盤** (共通 Core・MIR・Perceus・LLVM・ランタイム・FFI ABI・SMT 連携) を共有し、フロントエンドだけが異なります。

**現在は第一段階です。**

### 不変の設計原則

1. **実行時にトレーシング GC を持たない。** メモリは所有権 (線形/一意)・精密参照カウント (Perceus)・リージョンで管理する。
2. **純粋性を壊さない。** 副作用は型 (`IO` など) で表す。aCHC の拡張は既存 Haskell プログラムの意味を変えない (「`.hs` は `.js`、`.chs` は `.ts`」)。
3. **メモリ安全。** `unsafe` 相当の操作はランタイム/FFI 境界に限定する。リークは安全性違反ではないが (Rust と同じ)、検出・警告する。
4. **ネイティブ速度。** LLVM で機械語を生成し、Rust 相当の性能を目標とする。
5. **証明は実行時コストゼロ。** 型・証明・述語は多重度 0 で消去され、生成コードの性能を落とさない。
6. **共通基盤はフロントエンドに依存しない。** aCHC 固有・CirkaLang 固有の都合を共通基盤に持ち込まない。

### 版の運用 (CirkaLang)

- 開発中の版表示は `v0.0.N-dev`。リリースはその版の **リリースチケット** (例: FEAT-0207 = v0.0.1) で行う。
- リリース時: ベンチと評価メモを記録 → `CHANGELOG.md` を更新 → `cirkalang-*` クレートと `cirka` の版を更新 → タグ `cirkalang-v0.0.N` を打つ (タグとリリースの公開は利用者の確認を得てから)。
- プロトタイプ (v0.0.x) の間は版間の互換性を保証しない。`cirka.toml` の `edition` で対象版を明示する。

## 2. 必読ドキュメント

| パス | 内容 |
|------|------|
| `docs/specs/` | 元となる要求・構想(入力資料。原則編集しない) |
| `docs/design/00-overview.md` | 全体設計・主要な設計判断(ADR一覧) — **最初に読む** |
| `docs/design/01-architecture.md` | コンパイラのパイプラインとクレート構成 |
| `docs/design/02-language-extensions.md` | aCHC 言語仕様 (Haskell への拡張、スコープ縮小版) |
| `docs/design/03-type-system.md` | aCHC の型システム (多重度・一意性・Drop・借用。一意性等の規則は CirkaLang と共通) |
| `docs/design/04-memory-model.md` | メモリモデル(所有権・Perceus RC・リージョン・遅延評価) |
| `docs/design/05-runtime.md` | ランタイム(オブジェクト表現・アロケータ・起動・例外) |
| `docs/design/06-ir.md` | 中間表現 (共通 Core / MIR) |
| `docs/design/07-backend.md` | LLVMバックエンドとリンク |
| `docs/design/08-ffi.md` | C FFI |
| `docs/design/09-libraries.md` | ライブラリ戦略 (aCHC / CirkaLang プロトタイプ / 第二段階) |
| `docs/design/10-testing.md` | テスト戦略・互換性・性能計測 |
| `docs/design/11-roadmap.md` | トラック・マイルストーン・版とチケットの対応、優先順位 |
| `docs/design/12-cirkalang.md` | CirkaLang 言語設計 (プロトタイプ v0.0.x の各版と第二段階) |
| `docs/design/cirkalang/spec-v0.0.N.md` | CirkaLang 各版の言語仕様 (各版の仕様チケットで作成) |
| `docs/design/13-verification.md` | 検証機能 (依存型・全域性・部分集合型と SMT・契約・能力・効果・セッション型) |
| `docs/tasks/tasks.md` | チケット一覧(作業の起点) |

## 3. ディレクトリ構成

```
.
├── AGENTS.md               # 本ファイル
├── Cargo.toml              # Cargo ワークスペース
├── rust-toolchain.toml     # Rust ツールチェーン固定
├── Dockerfile / compose.yaml
├── .env.sample             # 環境依存設定の雛形 (.env にコピーして使う)
├── CHANGELOG.md            # CirkaLang の版ごとの変更 (v0.0.1 のリリース時に作成)
├── crates/                 # コンパイラ・ランタイム (Rust) ※詳細は 01-architecture.md
│   ├── cirka-diagnostics/  #   [共通] エラー表示・ソース位置
│   ├── cirka-core/         #   [共通] 共通 Core・Core Lint・Core 最適化
│   ├── cirka-mir/          #   [共通] MIR・Perceus (dup/drop)・検証器・インタプリタ
│   ├── cirka-codegen-llvm/ #   [共通] MIR → LLVM IR
│   ├── cirka-runtime/      #   [共通] ランタイム (staticlib, no_std)
│   ├── cirka-smt/          #   [共通] SMT (Z3) 連携
│   ├── cirka-bindgen/      #   [共通] C ヘッダ取り込み
│   ├── cirka-driver/       #   [共通] Core 以降のパイプライン・外部ツール
│   ├── achc-syntax/ achc-rename/ achc-typeck/ achc-desugar/   # [aCHC] Haskell フロントエンド
│   ├── achc/               #   [aCHC] CLI (バイナリ `achc`)
│   ├── cirkalang-syntax/ cirkalang-resolve/ cirkalang-elab/ cirkalang-check/ cirkalang-lower/  # [CirkaLang] フロントエンド
│   └── cirka/              #   [CirkaLang] CLI (バイナリ `cirka`)
├── lib/
│   ├── cirka-base/         #   [aCHC] base 互換ライブラリ (縮小版) + Cirka.* モジュール
│   └── cirkalang/          #   [CirkaLang] core / std
├── tests/                  # 統合テスト (mir / core / achc / cirkalang)
├── bench/                  # ベンチマーク (aCHC / CirkaLang / GHC / Rust 比較)
├── scripts/                # 開発用スクリプト (ci.sh など)
└── docs/
    ├── specs/              # 要求・構想
    ├── design/             # 設計書
    ├── tasks/
    │   ├── tasks.md        # チケット一覧
    │   ├── features/       # 機能チケット FEAT-XXXX-*.md
    │   └── bugs/           # バグチケット BUG-XXXX-*.md
    └── artifacts/          # AIが作る一時成果物 (.gitignore 済み)
```

まだ存在しないディレクトリは、該当チケットの作業時に作成します。

## 4. 開発環境

- ターゲットは **x86_64 Linux** のみ。ビルドは **Ubuntu 24.04 の Docker コンテナ上** を前提とする。
- 実装言語は **Rust** (edition 2024)。コンパイラ・ランタイムともRustで書く。
- 環境依存の値はすべて **`.env`** に置く。新しい環境変数を追加したら **必ず `.env.sample` にも追記** し、コメントで用途を書く。コードにパスやバージョンを直書きしない。

```sh
cp .env.sample .env            # 初回のみ。必要に応じて CIRKA_UID などを編集
docker compose build           # イメージ作成 (LLVM 18, GHC, Rust, Z3, valgrind, hyperfine)
docker compose run --rm dev    # コンテナ内シェル
# コンテナ内で
cargo build
cargo test
scripts/ci.sh                  # fmt / clippy / 全テスト (FEAT-0003 で作成)
```

コンテナ内の `ghc` は **差分テスト用の参照実装** であり、aCHC のビルドには使いません。

## 5. 作業フロー

1. **チケットから始める。** 作業は必ず `docs/tasks/features/` または `docs/tasks/bugs/` のチケットに紐づける。該当チケットがなければ先に作成し、`docs/tasks/tasks.md` の一覧に追加する。
2. **着手時** にチケットと `tasks.md` の状態を `in-progress` にする。
3. **テストを先に/同時に書く。** 受け入れ条件をテストで表現してから実装する。
4. **適切な粒度でコミットする**(後述)。
5. **完了時** にチケットの受け入れ条件をすべてチェックし、状態を `done` にして `tasks.md` を更新する。
6. 作業中に見つけたバグで、今のチケットの範囲外のものは `docs/tasks/bugs/` にバグチケットを起票する。
7. 設計を変える場合は、先に `docs/design/` を更新し、`00-overview.md` の設計判断(ADR)表に追記する。設計書と実装を乖離させない。

### チケット

- 機能: `docs/tasks/features/FEAT-XXXX-<kebab-title>.md`、バグ: `docs/tasks/bugs/BUG-XXXX-<kebab-title>.md`
- 番号はトラックごとの番号帯を使う: 共通基盤 0001–0099、aCHC 0100–0199、CirkaLang 0200–0299 (版ごとに 10 番ずつ)、第二段階 0300–。詳細は `11-roadmap.md`。
- 作業が競合したときの優先順位は `11-roadmap.md` §6 に従う (共通基盤 > CirkaLang v0.0.1–v0.0.3 > aCHC > 証明 > その他)。
- 雛形は `docs/tasks/features/_TEMPLATE.md` / `docs/tasks/bugs/_TEMPLATE.md`。
- 状態: `todo` → `in-progress` → `done`(他に `blocked` / `wontfix`)。
- `tasks.md` の一覧とチケット本文の状態は常に一致させる。

### 一時成果物

- 調査メモ・ログ・生成したIRのダンプ・ベンチ結果の生データなど、AIが作る一時的な成果物は **`docs/artifacts/`** に置く(git管理外)。
- 残す価値がある結論は、`docs/design/` やチケット本文に転記してからコミットする。

## 6. コミット規約

- **小さく、意味のある単位で** コミットする。1コミット1目的。テストが通る状態でコミットする。
- メッセージは Conventional Commits 形式 + チケットID:

  ```
  feat(achc-typeck): infer multiplicities for let bindings [FEAT-0120]
  feat(cirkalang-elab): add bidirectional checking for Pi types [FEAT-0203]
  fix(runtime): avoid stack overflow when freeing long lists [BUG-0003]
  docs(design): clarify Drop vs Release semantics
  test(mir): add golden tests for reuse analysis [FEAT-0022]
  ```
- type: `feat` `fix` `refactor` `perf` `test` `docs` `build` `chore`
- scope: クレート名 (`cirka-` は省略: `mir` `runtime` …、フロントエンドは `achc-typeck` `cirkalang-elab` のように接頭辞を残す)、または `design` `tasks` `docker` `release` など。
- 生成物(`target/`, `_build/`, `docs/artifacts/`, `.env`)はコミットしない。

## 7. テスト規約

**機能追加・バグ修正には必ずテストを付ける。** テストなしのコミットは原則不可。

| 種類 | 場所 | 内容 |
|------|------|------|
| ユニットテスト | 各クレートの `#[cfg(test)]` | 関数単位 |
| スナップショット | 各クレート `tests/snapshots` (insta) | パース結果・Core・MIR・LLVM IR のダンプ |
| 共通基盤 | `tests/mir/`, `tests/core/` | 手書き MIR / Core の実行 (インタプリタとネイティブの一致) |
| aCHC | `tests/achc/{run,fail,diff,compat}/` | 実行・診断・GHC との差分・互換計測 |
| CirkaLang | `tests/cirkalang/{run,fail,verify}/` | 実行 (版ごとのディレクトリ)・診断・証明の検証 (Z3 が必要) |
| メモリ | 実行テスト全体 | `CIRKA_LEAK_CHECK=1` でリーク 0、`CIRKA_ALLOC=system` + valgrind でエラー 0 |
| 性能 | `bench/` | Rust / GHC -O2 との比較 (hyperfine) |

詳細は `docs/design/10-testing.md`。

- 期待出力の更新は `ACHC_BLESS=1` / `CIRKA_BLESS=1` を付けて `cargo test` で行い、差分をレビューしてからコミットする。
- CirkaLang の古い版のテストも原則として通し続ける。意図的に壊す場合はリリースチケットと CHANGELOG に記録する。
- バグ修正には、そのバグを再現する回帰テストを必ず追加する。
- 実行テストはデフォルトで **リークチェック有効** で走らせる。リークが意図的な場合(循環構造の警告テスト等)はテスト側で明示する。

## 8. コーディング規約

- `cargo fmt` と `cargo clippy --all-targets -- -D warnings` を通す。
- **コンパイラはユーザーの誤りでパニックしない。** ユーザー起因のエラーは必ずソース位置付きの診断(`cirka-diagnostics`)で報告する。パニックは内部バグのみ(ICE として報告)。
- `unsafe` は `cirka-runtime` と FFI 境界に限定し、すべてに `// SAFETY:` コメントを書く。
- 各IR(Core / MIR)には **検証器(Lint)** を用意し、デバッグビルドでは各パスの後に必ず実行する。最適化パスは検証器を通る出力しか出してはならない。
- コード中のコメント・識別子は英語、ドキュメント(`docs/`)は日本語で書く。
- 外部クレートの追加は最小限にし、追加理由をコミットメッセージかチケットに書く。ライセンスは MIT / Apache-2.0 / BSD 互換に限る(本プロジェクトは MIT / Apache-2.0 デュアルライセンス)。

## 9. 完了の定義 (Definition of Done)

- [ ] チケットの受け入れ条件をすべて満たす
- [ ] 追加したテストを含め `scripts/ci.sh` がDockerコンテナ内で成功する
- [ ] リークチェック・IR検証器でエラーがない
- [ ] 関連する設計書・`tasks.md`・チケットの状態が更新されている
- [ ] 互換性や性能に影響する変更なら、計測結果をチケットに記録している
