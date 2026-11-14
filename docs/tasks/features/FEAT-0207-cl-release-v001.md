# FEAT-0207: CirkaLang v0.0.1 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.1 最小の正格言語・所有権・ネイティブ
- 優先度: P0
- 依存: FEAT-0206, FEAT-0016, FEAT-0022
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

v0.0.1 の評価とリリース。

## 作業内容

- [x] binary-trees / n-body の CirkaLang 版と Rust との比較
- [x] CHANGELOG と既知の制限
- [x] タグ `cirkalang-v0.0.1` (2026-11-14、利用者の確認を得てリリースのコミット `eade054` に注釈付きタグを打ち、push した)

## 受け入れ条件

- [x] 性能結果が `bench-report.md` に記録
- [x] 評価メモが次版の設計に反映されている
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- ベンチ: `bench/binary-trees/main.cirka`、`bench/n-body/main.cirka` (+ `bench/n-body/rust/main.rs`)。引数のために
  Prelude に `argCount` / `getArg` / `parseInt` を追加 (ランタイムの `cirka_argc` / `cirka_arg`)。
  `tests/cirkalang.rs` の `cirkalang_bench_programs` が小さい引数で Rust 版と同じ出力・リーク 0 を確認する。
- 結果 (`docs/design/bench-report.md`): binary-trees は Rust の 1.60 倍 (手書き MIR は 0.68 倍)、n-body は 52.9 倍。
  原因は数値のボックス化と配列の欠如。対策は FEAT-0041 (unboxing、v0.0.3) と FEAT-0211 (配列、v0.0.2) に記録した。
- 評価メモは `12-cirkalang.md` §8「v0.0.1 の評価」に記録。
- 版: ワークスペースの版を `0.0.1` にした (`cirka --version` は `CirkaLang v0.0.1`)。`CHANGELOG.md` を作成。
- タグ `cirkalang-v0.0.1` は、このリリースのコミット `eade054` に 2026-11-14 に打った (利用者の確認済み)。
