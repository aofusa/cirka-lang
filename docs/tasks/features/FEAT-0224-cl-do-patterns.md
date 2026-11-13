# FEAT-0224: do の構文の整理 (パターンの束縛)

- 状態: done
- トラック: CirkaLang
- マイルストーン: CirkaLang v0.0.3 借用・リージョン・性能
- 優先度: P1
- 依存: FEAT-0215
- 設計: [spec-v0.0.3.md](../../design/cirkalang/spec-v0.0.3.md) §5

## 概要

v0.0.2 の評価 (12-cirkalang.md) で不便とされた `do` の構文を整理する: `_ <-` で結果を捨てられない、組のパターン束縛 `(a, b) <-` がない、
`do` の中の `let` に複数行の `case` を書けない。

## 作業内容

- [x] `p <- e` (変数以外のパターン) を、残りの文を `case` の選択肢に入れる形に変換する (解決器)
- [x] 反駁可能なパターンの残りの場合 `p <- e | q => h` (Idris と同じ)
- [x] `do` の `let` の複数行の `case` (v0.0.2 のレイアウト規則で既に書けることを確認し、テストに入れた)

## 受け入れ条件

- [x] 実行テスト `run/v003/do_patterns` (線形な `File` の組の分解を含む)、診断 `fail/type/do_pattern_missing`、構文 `parse/do_patterns`
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

- 設計: ADR-075。`_ <- e` は `case` の `_` なので、線形な結果は従来どおり使い切る必要がある (`Release` を持つ値は自動で解放される)。
