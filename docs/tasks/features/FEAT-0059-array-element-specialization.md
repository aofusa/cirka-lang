# FEAT-0059: 配列要素の単相化 (`UElem` の特殊化)

- 状態: todo
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0041, FEAT-0056
- 設計: [16-performance.md](../../design/16-performance.md) §2、[achc-evaluation.md](../../design/achc-evaluation.md) §2.1

## 概要

`Cirka.Array.Unboxed` の要素の読み書きは `UElem` クラスのメソッドを通り、特殊化されずに要素が箱に入って返る
(spectral-norm の `.chs` が遅い原因。bench-report §FEAT-0043)。要素型ごとに単相化し、読み書きを箱なしのプリミティブにする。
CirkaLang の配列 (FEAT-0211) も同じ仕組みに乗せる。

## 作業内容

- [ ] `UElem` の辞書を使う関数を、呼び出し側の要素型で特殊化する (辞書が既知の大域定数になる場合)
- [ ] 特殊化後の読み出しの結果を、呼び出し側の `case` で箱なしのまま受け取る (CPR)
- [ ] spectral-norm `.chs` を計測し bench-report に記録

## 受け入れ条件

- [ ] `UElem` を使うループの確保回数が要素数によらないことを `-- max-allocs` で確かめるテスト
- [ ] テストを追加し `scripts/ci.sh` が通る

## メモ
