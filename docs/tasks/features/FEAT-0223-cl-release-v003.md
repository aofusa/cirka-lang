# FEAT-0223: CirkaLang v0.0.3 リリース

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P0
- 依存: FEAT-0220, FEAT-0221, FEAT-0222, FEAT-0224, FEAT-0041
- 設計: [11-roadmap.md](../../design/11-roadmap.md)

## 概要

性能目標 (Rust の 1.5 倍以内) の評価とリリース。

## 作業内容

- [x] 全ベンチの CirkaLang 版と Rust の比較 (bench-report.md の「CirkaLang v0.0.3」)
- [x] CHANGELOG
- [ ] タグ `cirkalang-v0.0.3` (利用者の確認を得てから打つ。AGENTS.md)

## 受け入れ条件

- [x] G3 の達成状況と不足分の対策チケット (7 題材中 5 つで達成。cli-grep 1.91 / string-split 1.60 は FEAT-0082 / FEAT-0080)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 版: `Cargo.toml` のワークスペースの版、`cirka` の `EDITION`、パッケージのテストの `edition`、E1004 の文言を 0.0.3 にした。
- `tests/workspace.rs` は差分の仕様の連鎖 (v0.0.3 → v0.0.2 → v0.0.1) をたどって全体の節を確かめる。
- n-body の CirkaLang 版を一意な `UArray` と借用で書き直した (0.57 s、Rust 0.39 s)。
- 評価メモは 12-cirkalang.md の「v0.0.3 の評価」。
