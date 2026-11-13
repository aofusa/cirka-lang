# FEAT-0080: Core の結合点 (join point)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P2
- 依存: FEAT-0079
- 設計: [16-performance.md](../../design/16-performance.md) §4

## 概要

`let! x = case s of A -> K a | B -> K b in k` の `k` は、Core に結合点がないため、case の結果を箱 (`K`) として受け取る。
case-of-case で `k` を腕に複製すると大きくなるので、現在は箱を作ってから分岐し直している (FEAT-0079 の残り。`bench/string-split` で 1 行あたり 2 回の確保)。

GHC と同じく、`k` を箱なしの値を受け取る局所関数 (結合点: 必ず末尾で呼ばれ、クロージャを作らない) にし、各腕から `jump j a` で呼ぶ。

## 作業内容

- [ ] Core に結合点 (`join j xs = e in body` と `jump`) を加え、Lint で末尾呼び出しだけであることを検査する
- [ ] case-of-case と `let!` の case で、大きい継続を結合点にする
- [ ] MIR への変換: 結合点はブロックと引数 (φ) にする
- [ ] 単体テストと実行テスト (確保の回数)

## 受け入れ条件

- [ ] `bench/string-split` の確保が 1 行あたり 1 回以下
- [ ] 全ベンチの出力が変わらず、時間が増えない
- [ ] `scripts/ci.sh` が通る

## メモ
