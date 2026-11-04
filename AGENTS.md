# AGENTS.md — CirkaLang / aCHC 開発ガイド

このファイルは、本リポジトリで作業するAIエージェントおよび人間の開発者向けの作業規約です。
作業を始める前に必ず全体を読んでください。

## 1. プロジェクト概要

**CirkaLang** は「Haskellの型システムと純粋性を維持したまま、GCを使わず、所有権・RAII・
メモリ安全性をRust並みに備え、ネイティブ速度で動く言語」を目指すプロジェクトです。
イメージは *Linear/Uniqueness ベースの関数型Rust* です。

開発は2段階で進めます。

| 段階 | 成果物 | 内容 |
|------|--------|------|
| 第一段階 | **aCHC** (aofusa's CirkaHaskellCompiler) | Haskellの上位互換拡張として試作。既存Haskellコード(`.hs`)はそのままコンパイル・実行でき、拡張(`.chs` / `{-# LANGUAGE CirkaOwnership #-}`)で線形性・一意性・RAII・借用を使える。JavaScriptに対するTypeScriptの関係を目指す |
| 第二段階 | **CirkaLang** | 第一段階の検証結果をもとに、Haskellとの互換性を捨てて再設計した独自言語 |

**現在は第一段階(aCHC)です。**

### 不変の設計原則

1. **実行時にトレーシングGCを持たない。** メモリは所有権(線形/一意)・精密参照カウント(Perceus)・リージョンで管理する。
2. **Haskellの純粋性と型システムを壊さない。** 副作用は `IO` に閉じ込める。拡張は既存プログラムの意味を変えない(「`.hs` は `.js`、`.chs` は `.ts`」)。
3. **メモリ安全。** `unsafe` 相当の操作はランタイム/FFI境界に限定する。リークは安全性違反ではないが(Rustと同じ)、検出・警告する。
4. **ネイティブ速度。** LLVMで機械語を生成し、Rust相当の性能を目標とする。

## 2. 必読ドキュメント

| パス | 内容 |
|------|------|
| `docs/specs/` | 元となる要求・構想(入力資料。原則編集しない) |
| `docs/design/00-overview.md` | 全体設計・主要な設計判断(ADR一覧) — **最初に読む** |
| `docs/design/01-architecture.md` | コンパイラのパイプラインとクレート構成 |
| `docs/design/02-language-extensions.md` | aCHC言語仕様(Haskellへの拡張) |
| `docs/design/03-type-system.md` | 型システム(多重度・一意性・Drop・借用)の規則 |
| `docs/design/04-memory-model.md` | メモリモデル(所有権・Perceus RC・リージョン・遅延評価) |
| `docs/design/05-runtime.md` | ランタイム(オブジェクト表現・アロケータ・起動・例外) |
| `docs/design/06-ir.md` | 中間表現(Core / MIR) |
| `docs/design/07-backend.md` | LLVMバックエンドとリンク |
| `docs/design/08-ffi.md` | C FFI |
| `docs/design/09-libraries.md` | ライブラリ戦略(第一段階/第二段階) |
| `docs/design/10-testing.md` | テスト戦略・互換性・性能計測 |
| `docs/design/11-roadmap.md` | マイルストーンとチケットの対応 |
| `docs/design/12-cirkalang.md` | 第二段階 CirkaLang の方向性 |
| `docs/tasks/tasks.md` | チケット一覧(作業の起点) |

## 3. ディレクトリ構成

```
.
├── AGENTS.md               # 本ファイル
├── Cargo.toml              # Cargo ワークスペース
├── rust-toolchain.toml     # Rust ツールチェーン固定
├── Dockerfile / compose.yaml
├── .env.sample             # 環境依存設定の雛形 (.env にコピーして使う)
├── crates/                 # コンパイラ・ランタイム (Rust) ※詳細は 01-architecture.md
│   ├── achc/               #   ドライバ (バイナリ `achc`)
│   ├── cirka-syntax/       #   字句解析・レイアウト・構文解析
│   ├── cirka-rename/       #   名前解決・モジュール
│   ├── cirka-typeck/       #   型検査・推論(型クラス・多重度・一意性)
│   ├── cirka-core/         #   Core IR・Core Lint・Core最適化
│   ├── cirka-desugar/      #   AST → Core
│   ├── cirka-mir/          #   MIR・所有権(dup/drop)挿入・検証・インタプリタ
│   ├── cirka-codegen-llvm/ #   MIR → LLVM IR
│   ├── cirka-runtime/      #   ランタイム (staticlib, no_std)
│   └── cirka-diagnostics/  #   エラー表示・ソース位置
├── lib/                    # Haskell(aCHC)で書かれたライブラリ
│   └── cirka-base/         #   base 互換ライブラリ (Prelude 等) + Cirka.* モジュール
├── tests/                  # 統合テスト (run / fail / diff / compat)
├── bench/                  # ベンチマーク (aCHC / GHC / Rust 比較)
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
docker compose build           # イメージ作成 (LLVM 18, GHC, Rust, valgrind, hyperfine)
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
- 番号は連番。機能チケットはマイルストーンごとに番号帯を分けている(`11-roadmap.md` 参照)。
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
  feat(typeck): infer multiplicities for let bindings [FEAT-0050]
  fix(runtime): avoid stack overflow when freeing long lists [BUG-0003]
  docs(design): clarify Drop vs Release semantics
  test(mir): add golden tests for reuse analysis [FEAT-0022]
  ```
- type: `feat` `fix` `refactor` `perf` `test` `docs` `build` `chore`
- scope: クレート名から `cirka-` を除いたもの(`syntax` `typeck` `mir` `runtime` …)、または `design` `tasks` `docker` など。
- 生成物(`target/`, `_build/`, `docs/artifacts/`, `.env`)はコミットしない。

## 7. テスト規約

**機能追加・バグ修正には必ずテストを付ける。** テストなしのコミットは原則不可。

| 種類 | 場所 | 内容 |
|------|------|------|
| ユニットテスト | 各クレートの `#[cfg(test)]` | 関数単位 |
| スナップショット | 各クレート `tests/snapshots` (insta) | パース結果・Core・MIR・LLVM IR のダンプ |
| 実行テスト | `tests/run/**/*.hs|.chs` + `.stdout` (+ `.stdin`, `.exitcode`) | コンパイル→実行→出力比較 |
| エラーテスト | `tests/fail/**/*.hs|.chs` + `.stderr` | 型エラー・所有権エラーの診断メッセージ |
| 差分テスト | `tests/diff/**/*.hs` | 同じプログラムを GHC と aCHC で実行して比較 |
| 互換性 | `tests/compat/` | GHC testsuite / nofib の一部を取り込み、合格率を記録 |
| メモリ | 実行テスト全体 | `CIRKA_LEAK_CHECK=1` でリーク0、`CIRKA_ALLOC=system` + valgrind でエラー0 |
| 性能 | `bench/` | GHC -O2 / Rust との比較 (hyperfine) |

- 期待出力の更新は `ACHC_BLESS=1 cargo test` で行い、差分をレビューしてからコミットする。
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
