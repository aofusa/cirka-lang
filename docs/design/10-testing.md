# 10. テスト戦略

正しさ、メモリ安全性 (リーク・不正アクセスなし)、性能 (Rust / GHC 比) を継続的に計測する。
正しさの基準は、aCHC では「GHC と同じ結果」、CirkaLang では「期待出力と証明の検査結果」。
すべて Docker コンテナ内の `scripts/ci.sh` で実行できること。

## 1. テストの種類と配置

| 種類 | 配置 | 対象 | 判定 |
|------|------|------|------|
| ユニット | 各クレート `#[cfg(test)]` | 全部 | |
| スナップショット | 各クレート `tests/` + `insta` | 全部 | AST / TT / Core / MIR / LLVM IR のダンプが期待どおり |
| MIR 実行 | `tests/mir/*.mir` + `.stdout` | 共通基盤 | インタプリタとネイティブの出力・確保/解放回数が一致 |
| Core 実行 | `tests/core/*.core` + `.stdout` | 共通基盤 | 手書き Core が共通ドライバで動く |
| aCHC 実行 | `tests/achc/run/**/*.{hs,chs}` + `.stdout` (`.stdin`, `.args`, `.exitcode`) | aCHC | 出力一致、debug ランタイムでリーク 0 |
| aCHC エラー | `tests/achc/fail/**/*.{hs,chs}` + `.stderr` | aCHC | 診断コードとメッセージが一致 |
| aCHC 差分 | `tests/achc/diff/**/*.hs` | aCHC | GHC の実行結果と stdout・終了コードが一致 (`CIRKA_GHC` 未設定ならスキップ) |
| aCHC 互換計測 | `tests/achc/compat/` (GHC testsuite / nofib の一部) | aCHC | 合格率は目標にせず、失敗理由の分類を記録 (ADR-025) |
| CirkaLang 実行 | `tests/cirkalang/run/v0.0.N/**/*.cirka` + `.stdout` | CirkaLang | 出力一致、リーク 0。版ごとにディレクトリを分ける |
| CirkaLang エラー | `tests/cirkalang/fail/**/*.cirka` + `.stderr` | CirkaLang | 型・所有権・全域性・証明の診断が一致 |
| CirkaLang 検証 | `tests/cirkalang/verify/{pass,fail}/**/*.cirka` | CirkaLang (v0.0.4〜) | 証明が通る / 反例付きで失敗する (`CIRKA_Z3` 未設定ならスキップ) |
| 契約の実行時検査 | `tests/cirkalang/run/contracts/` | CirkaLang | `--contracts=check` で違反時に期待どおり停止 |
| メモリ | 実行テスト全体 | 全部 | `CIRKA_ALLOC=system` + valgrind でエラー 0 |
| 並行性 | `tests/cirkalang/run/threads/` | CirkaLang (v0.0.5〜) | ThreadSanitizer でエラーなし |
| 性能 | `bench/` | 全部 | hyperfine で比較し記録 |
| ファズ | `fuzz/` (cargo-fuzz、後半) | 両パーサ・型検査 | パニックしない |

## 2. テストハーネス (`tests/harness`)

- 各テストファイル先頭のコメントでオプションを指定できる:
  ```
  -- flags: -O2 --ownership=strict
  -- runtime-env: CIRKA_STACK_SIZE=64M
  -- expect-leak: 1          (循環の警告テストなど、意図的なリーク)
  -- skip-backend: llvm      (インタプリタのみで実行)
  ```
- `ACHC_BLESS=1` / `CIRKA_BLESS=1` で期待ファイルを更新。
- 失敗時は生成した `.ll`・ダンプを `_build/test-failures/<name>/` に残す。
- 並列実行 (`CIRKA_TEST_JOBS`)。
- CirkaLang のテストは版ごとのディレクトリに置き、古い版のテストも原則として通し続ける。
  版の変更で意図的に壊す場合は、リリースチケットと CHANGELOG に記録してテストを更新する。

## 3. aCHC: GHC との差分テスト

GHC (Docker イメージに同梱、Ubuntu 24.04 の `ghc` パッケージ) を参照実装として使う。

1. `ghc -O0 Foo.hs -o foo.ghc && ./foo.ghc < stdin` の結果を `_build/ghc-cache/` にキャッシュ。
2. aCHC でコンパイル・実行した結果と比較。
3. 差分が出たら、まず MIR インタプリタで再現するか確認し、フロントエンド/バックエンドどちらのバグかを切り分ける。

## 4. aCHC: 互換性の計測 (縮小)

- GHC の `testsuite/tests/**/should_run/*.hs` と `nofib/{imaginary,spectral}` の一部を取り込み (BSD ライセンス表記を保持)、
  失敗理由 (パース / 未対応拡張 / 型検査 / 実行時差分 / リーク / 性能) を分類する。
- 合格率は目標にしない (ADR-025)。分類結果は「GC なし実行で何が問題になるか」「CirkaLang で何を捨てるか」の材料として
  `docs/design/compat-report.md` と aCHC 評価レポート (FEAT-0132) に記録する。

## 5. 性能の計測

`bench/<name>/` に同じアルゴリズムの実装を並べる:

```
bench/binary-trees/
  plain.hs        -- 素の Haskell (GHC -O2 と aCHC で比較)
  own.chs         -- 一意性・借用を使った aCHC 版
  main.cirka      -- CirkaLang 版
  mir/            -- 手書き MIR (M1 の基礎性能検証)
  rust/main.rs    -- Rust 版 (基準)
```

- 題材: binary-trees, n-body, spectral-norm, fannkuch-redux, mandelbrot, fasta、配列の in-place 更新、文字列処理。
  CirkaLang の実用版の題材: CUI ツール (v0.0.2)、HTTP サーバ (v0.0.5)。
- 目標 (00-overview G3): `main.cirka` と `own.chs` が Rust の 1.5 倍以内、`plain.hs` が GHC -O2 と同等。
- 計測値 (時間・最大 RSS) はチケットと `docs/design/bench-report.md` に記録する。CirkaLang は版ごとの推移も記録する。
- v0.0.4 以降は、証明付きコードと証明なしコードの性能が同じであること (消去の確認) も計測する。

## 6. CI スクリプト (`scripts/ci.sh`)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build -p cirka-runtime --profile release   (+ debug runtime)
cargo test --workspace                           (ユニット・スナップショット・mir・core・achc run/fail・cirkalang run/fail)
cargo test --test achc_diff                      (CIRKA_GHC があれば)
cargo test --test cirkalang_verify               (CIRKA_Z3 があれば)
```

`scripts/memcheck.sh`、`scripts/compat.sh`、`scripts/bench.sh` は時間がかかるため任意実行 (マイルストーン完了時・CirkaLang のリリース時は必須)。
