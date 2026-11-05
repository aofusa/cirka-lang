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
| aCHC 実行 | `tests/achc/run/**/*.{hs,chs}` + `.stdout` (`.stdin`, `.args`, `.exitcode`, `.stderr`, `.warn`)。`Main.hs` のあるディレクトリは複数モジュールの一つのプログラム (他のモジュールはテストにしない) | aCHC | 出力一致、debug ランタイムでリーク 0 |
| aCHC エラー | `tests/achc/fail/**/*.{hs,chs}` + `.stderr` | aCHC | 診断コードとメッセージが一致 |
| aCHC 差分 | `tests/achc/diff/**/*.hs` + `.stdout` (`.exitcode`) | aCHC | 期待値は GHC 9.4 (`-O0`) の出力。実行テストと同じ検査 (リーク 0・ネイティブとの一致) を -O2 だけで行い、`CIRKA_GHC` があれば GHC とも比べる (FEAT-0117) |
| aCHC 互換計測 | `tests/achc/compat/` (GHC testsuite / nofib の一部) | aCHC | 合格率は目標にせず、失敗理由の分類を記録 (ADR-025) |
| aCHC 検証 (後期) | `tests/achc/verify/{pass,fail}/**/*.{hs,chs}` | aCHC (H6〜) | 洗練型・契約・全域性が通る / 反例付きで失敗する (`CIRKA_Z3` 未設定ならスキップ) |
| CirkaLang 実行 | `tests/cirkalang/run/v0.0.N/**/*.cirka` + `.stdout` | CirkaLang | 出力一致、リーク 0。版ごとにディレクトリを分ける |
| CirkaLang エラー | `tests/cirkalang/fail/**/*.cirka` + `.stderr` | CirkaLang | 型・所有権・全域性・証明の診断が一致 |
| CirkaLang 検証 | `tests/cirkalang/verify/{pass,fail}/**/*.cirka` | CirkaLang (v0.0.4〜) | 証明が通る / 反例付きで失敗する (`CIRKA_Z3` 未設定ならスキップ) |
| 契約の実行時検査 | `tests/cirkalang/run/contracts/` | CirkaLang | `--contracts=check` で違反時に期待どおり停止 |
| メモリ | 実行テスト全体 | 全部 | `CIRKA_ALLOC=system` + valgrind でエラー 0 |
| 並行性 | `tests/cirkalang/run/threads/` | CirkaLang (v0.0.5〜) | ThreadSanitizer でエラーなし |
| Vulkan compute | `tests/cirkalang/run/v0.0.6/vulkan/` | CirkaLang (v0.0.6〜) | lavapipe 上で CPU 計算と一致、検証レイヤーのエラー・警告 0 (Vulkan がなければスキップ) |
| 遠隔 GUI (任意) | `scripts/remote.sh` で手動実行 | macOS / Windows | CI には含めない。結果は `docs/artifacts/remote/` に置き、結論をチケットに記録 (§7) |
| 性能 | `bench/` | 全部 | hyperfine で比較し記録 |
| ファズ | `fuzz/` (cargo-fuzz、後半) | 両パーサ・型検査 | パニックしない |

## 2. テストハーネス (`tests/harness`)

- 各テストファイル先頭のコメントでオプションを指定できる:
  ```
  -- flags: -O2 --ownership=strict
  -- runtime-env: CIRKA_STACK_SIZE=64M
  -- expect-leak: 1          (循環の警告テストなど、意図的なリーク)
  -- skip-backend: llvm      (インタプリタのみで実行)
  -- opt-levels: 2           (aCHC の run テスト: 指定した最適化レベルだけで実行。既定は 0 と 2)
  -- max-allocs: 3001000     (ネイティブ実行の確保回数の上限。CIRKA_STATS の allocs)
  -- max-forced: 1001000     (ネイティブ実行で評価されたサンクの数の上限。CIRKA_STATS の forced)
  -- max-array-copies: 0     (in-place 更新で共有配列をコピーした回数の上限。CIRKA_STATS の array_copies)
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

## 4. aCHC: 互換性の計測

- GHC の `testsuite/tests/**/should_run/*.hs` と `nofib/{imaginary,spectral}` の一部を取り込み (BSD ライセンス表記を保持)、
  失敗理由 (パース / 未対応拡張 / 型検査 / 実行時差分 / リーク / 性能) を分類する。
- 合格率は目標にしない (ADR-025)。分類結果は「GC なし実行で何が問題になるか」「CirkaLang で何を捨てるか」の材料として
  `docs/design/compat-report.md` と aCHC 評価レポート (前期 FEAT-0132、後期 FEAT-0164) に記録する。
- 後期 (H5) では GADTs・型族などを使う GHC testsuite のテストも計測対象に加え、前期との差を記録する。
- 実装 (FEAT-0119): `scripts/import-compat.py` が GHC 9.4.7 の testsuite から選んだプログラムを `tests/achc/compat/ghc/`
  に取り込む (LICENSE 同梱)。nofib は単一のライセンスを持たないためコミットせず、`CIRKA_NOFIB_DIR` から
  `_build/compat/nofib` に取り込む。`scripts/compat.sh` (`tests/achc_compat.rs` の ignored テスト) が各プログラムを
  子プロセスでビルド・実行し (コンパイラの異常終了も 1 本の分類で済む)、`docs/artifacts/compat/` に結果を書く。
  `tests/achc/compat/baseline.tsv` で `pass` だったものが落ちると失敗する (回帰検出)。分類器は通常の `cargo test` で検査する。

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
cargo test --workspace の achc_diff               (GHC との比較は CIRKA_GHC があれば)
cargo test --test cirkalang_verify               (CIRKA_Z3 があれば)
cargo test --test achc_verify                    (後期 H6〜、CIRKA_Z3 があれば)
```

`scripts/memcheck.sh`、`scripts/compat.sh`、`scripts/bench.sh` は時間がかかるため任意実行 (マイルストーン完了時・CirkaLang のリリース時は必須)。

## 7. 遠隔環境での GUI 検証 (任意)

- Docker 内では GUI を扱わない (ADR-028)。ウィンドウを使う検証は、利用者が `.env` に設定した macOS / Windows のホストで `scripts/remote.sh` を使って手動で行う ([15-gpu-and-remote.md](15-gpu-and-remote.md), FEAT-0052, 0290)。
- 接続先・作業ディレクトリは `.env` の `CIRKA_REMOTE_*` からのみ読む (ADR-029)。値が空なら何もしない。
- 判定はオフスクリーン描画の画像比較と検証レイヤーのログ。ログ・画像は `docs/artifacts/remote/<mac|win>/` に回収し、結論だけをチケットに記録する。
- CI・リリース条件には含めない。
