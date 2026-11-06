# FEAT-0059: 配列要素の単相化 (`UElem` の特殊化)

- 状態: done
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

- [x] `UElem` の辞書を使う関数を、呼び出し側の要素型で特殊化する (BUG-0011 の多相な辞書の解決で実現済みだった)
- [x] 特殊化後の読み出しの結果を、呼び出し側の `case` で箱なしのまま受け取る (`bytes_getf` が直接インライン化される)
- [x] spectral-norm `.chs` を調べ、残りの確保の原因を特定した (下記。FEAT-0061 へ)

## 受け入れ条件

- [x] `UElem` を使うループの確保回数が要素数によらないことを `-- max-allocs` で確かめるテスト (`tests/achc/run/h3/uarray_elem_loop.hs`: 20 万回の読み書きで確保 91 回)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ
- 着手時に調べると、`UElem` のメソッドは BUG-0011 (多相な辞書 `Λr. …$Dict` の解決とフィールドの浮上) の後、要素型で特殊化されて
  `bytes_getf` / `bytes_setf` まで直接インライン化されていた。読み書きのループは要素あたりの確保 0。
- spectral-norm `own.chs` に残る確保 (内側の反復 1 回あたり約 3 回、n=100 で 61 万回) の原因は `UElem` ではなく、**関数を引数に取る再帰関数**
  (`mul f n u` の `go`、`U.generate n f` のループ) が `f` を未知の関数として呼ぶことにある。引数の `I#` と結果の `D#` が毎回箱になる。
  再帰の間で変わらない関数引数 (静的引数) での特殊化が必要で、FEAT-0061 (SpecConstr) で扱う。
