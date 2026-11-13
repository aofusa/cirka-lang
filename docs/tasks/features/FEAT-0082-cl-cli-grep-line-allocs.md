# FEAT-0082: 行ごとの畳み込みの確保の除去 (cli-grep)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: FEAT-0223
- 設計: [16-performance.md](../../design/16-performance.md) §4

## 概要

`bench/cli-grep` は Rust の 1.91 倍 (CirkaLang v0.0.3、bench-report.md)。100 万行で 200 万回の確保があり、
1 行あたり 2 回: `Data.String.foldLines` が渡す行のビュー `Str` と、`step` が返す累積値のレコード `MkCount` と考えられる。

## 作業内容

- [ ] 確保の内訳を MIR の統計で確かめる
- [ ] 高階の畳み込み (`foldLines` / `foldWords`) を関数引数ごとに特殊化し、累積値と行のビューを worker/wrapper で unbox する
- [ ] 単体テストと実行テスト (確保の回数)

## 受け入れ条件

- [ ] `bench/cli-grep` の確保が 1 行あたり 1 回未満、Rust の 1.5 倍以内
- [ ] 全ベンチの出力が変わらず、時間が増えない
- [ ] `scripts/ci.sh` が通る

## メモ
