# 10. テスト戦略

正しさ(GHC と同じ結果)、メモリ安全性(リーク・不正アクセスなし)、性能(Rust/GHC 比)の3軸で継続的に計測する。
すべて Docker コンテナ内の `scripts/ci.sh` で実行できること。

## 1. テストの種類と配置

| 種類 | 配置 | 実行 | 判定 |
|------|------|------|------|
| ユニット | 各クレート `#[cfg(test)]` | `cargo test` | |
| スナップショット | 各クレート `tests/` + `insta` | `cargo test` | AST / Core / MIR / LLVM IR のダンプが期待どおり |
| MIR 実行 | `tests/mir/*.mir` + `.stdout` | `cargo test -p cirka-mir` / 統合 | インタプリタとネイティブの出力・確保/解放回数が一致 |
| 実行 (run) | `tests/run/**/*.{hs,chs}` + `.stdout` (`.stdin`, `.args`, `.exitcode`) | `cargo test --test run` | 出力一致、debug ランタイムでリーク 0 |
| エラー (fail) | `tests/fail/**/*.{hs,chs}` + `.stderr` | `cargo test --test fail` | 診断コードとメッセージが一致 |
| 差分 (diff) | `tests/diff/**/*.hs` | `cargo test --test diff` (`CIRKA_GHC` 未設定ならスキップ) | GHC の実行結果と stdout・終了コードが一致 |
| 互換 (compat) | `tests/compat/` (GHC testsuite / nofib から取り込み) | `scripts/compat.sh` | 合格率を `docs/design/compat-report.md` に記録 (下がったら失敗) |
| メモリ | 実行テスト全体 | `scripts/memcheck.sh` | `CIRKA_ALLOC=system` + valgrind でエラー 0 |
| 性能 | `bench/` | `scripts/bench.sh` | hyperfine で aCHC / GHC -O2 / Rust を比較、結果を記録 |
| ファズ | `fuzz/` (cargo-fuzz、後半) | 手動/定期 | パーサ・型検査がパニックしない |
| 検証 (verify) | `tests/verify/{pass,fail}/**/*.{hs,chs}` (+ `.stderr`) | `cargo test --test verify` (`CIRKA_Z3` 未設定ならスキップ) | 検証が通る/期待どおりの反例付きエラーになる。Liquid Haskell の公開ベンチの一部も取り込む |
| 契約の実行時検査 | `tests/run/contracts/` | `--contracts=check` で実行 | 違反時に期待どおりのメッセージで停止 |

## 2. テストハーネス (`tests/harness`)

- 各テストファイル先頭のコメントでオプションを指定できる:
  ```haskell
  -- achc-flags: -O2 --ownership=strict
  -- runtime-env: CIRKA_STACK_SIZE=64M
  -- expect-leak: 1          (循環の警告テストなど、意図的なリーク)
  -- skip-backend: llvm      (インタプリタのみで実行)
  ```
- `ACHC_BLESS=1` で期待ファイルを更新。
- 失敗時は生成した `.ll`・ダンプを `_build/test-failures/<name>/` に残す。
- 並列実行 (`CIRKA_TEST_JOBS`)。

## 3. GHC との差分テスト

GHC (Docker イメージに同梱、Ubuntu 24.04 の `ghc` パッケージ) を参照実装として使う。

1. `ghc -O0 Foo.hs -o foo.ghc && ./foo.ghc < stdin` の結果を `_build/ghc-cache/` にキャッシュ。
2. aCHC でコンパイル・実行した結果と比較。
3. 差分が出たら、まず MIR インタプリタで再現するか確認し、フロントエンド/バックエンドどちらのバグかを切り分ける。

差分テストの題材: 自作の小さなプログラム、QuickCheck 風のランダム生成プログラム(後半、FEAT-0079)。

## 4. 互換性の計測

- GHC の `testsuite/tests/**/should_run/*.hs`(`.stdout` 付き)と `nofib/{imaginary,spectral,real}` の一部を `tests/compat/` に取り込む
  (BSD ライセンス表記を保持)。
- 取り込み時に「使用している拡張」「依存モジュール」を自動分類し、失敗理由(パース/未対応拡張/型検査/実行時差分/リーク)を集計する。
- 合格率は `docs/design/compat-report.md` に記録し、退行を CI で検出する。

## 5. 性能の計測

`bench/<name>/` に同じアルゴリズムの3実装を置く:

```
bench/binary-trees/
  plain.hs        -- 素の Haskell (GHC と aCHC で比較)
  cirka.chs       -- 一意性・借用を使った aCHC 版
  rust/main.rs    -- Rust 版
```

題材: binary-trees, n-body, spectral-norm, fannkuch-redux, mandelbrot, fasta, k-nucleotide(後半), 自作の配列 in-place 更新・文字列処理。
目標 (00-overview の G3): `cirka.chs` が Rust の 1.5 倍以内、`plain.hs` が GHC -O2 と同等。
計測値(時間・最大 RSS)はチケットと `docs/design/bench-report.md` に記録する。

## 6. CI スクリプト (`scripts/ci.sh`)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build -p cirka-runtime --profile release   (+ debug runtime)
cargo test --workspace                           (ユニット・スナップショット・run・fail・mir)
cargo test --test diff                           (CIRKA_GHC があれば)
cargo test --test verify                         (CIRKA_Z3 があれば)
```

`scripts/memcheck.sh`、`scripts/compat.sh`、`scripts/bench.sh` は時間がかかるため任意実行(マイルストーン完了時は必須)。
