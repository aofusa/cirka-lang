# FEAT-0074: 比較の結果を束縛してから分岐する Core を case-of-case にかける

- 状態: done
- トラック: 共通基盤
- マイルストーン: M2
- 優先度: P1
- 依存: FEAT-0013
- 設計: [06-ir.md](../../design/06-ir.md), [16-performance.md](../../design/16-performance.md) §4

## 概要

CirkaLang の `if` は Core で次の形になる (`scrut$` は `elab.rs` が作る束縛)。

```
(let! (scrut$ Bool (case (prim lt a b) (c Int#) Bool (0 (con False ())) (_ (con True ()))))
  (case scrut$ (s Bool) r ((False) e1) ((True) e2)))
```

`case` の scrutinee が変数なので case-of-case が働かず、比較の結果の `Bool` をいったん作ってから読み直す。Mandelbrot の `main.cirka` の内側のループは 1 反復あたり
2 回これを行い、Rust の 4 倍の命令を実行している (FEAT-0073 の版、`n = 300` で 134.6 M 対 33.0 M)。

`let!` の変数を使うのが直後の `case` の scrutinee だけで、右辺の `case` に case-of-case が適用できるなら、`let!` を畳んで `case (case ..) of ..` にする。
次の反復の case-of-case が `Bool` を消す。評価の順序は変わらない。

## 作業内容

- [x] `opt.rs`: `let!` の右辺が `case` で、変数の使用が直後の `case` だけのときに畳む (条件は case-of-case と同じ `case_of_case_ok`)
- [x] Core の単体テストと実行テスト、CirkaLang の実行テスト
- [x] ベンチで前後を計測する

## 受け入れ条件

- [x] 比較から作った `Bool` への分岐で `Bool` を作らないテスト
- [x] 変数の他の使用がある場合と右辺が呼び出しの場合は畳まないテスト
- [x] `main.cirka` の Mandelbrot の命令数が減る (計測をメモに記録)
- [x] テストを追加し `scripts/ci.sh` が通る

## メモ

callgrind の命令数 (`main.cirka`):

| ベンチ | FEAT-0073 | FEAT-0074 | Rust |
|--------|---:|---:|---:|
| mandelbrot (300) | 134.6 M | **33.5 M** | 33.0 M |
| spectral-norm (100) | 119.3 M | **20.0 M** | - |
| n-body (50,000) | 592.5 M | 592.5 M | - |
| binary-trees (10) | 45.3 M | 45.3 M | - |

- valgrind の下では `mmap` の予約に失敗してランタイムが `malloc` に切り替わるので、確保の多いベンチの命令数には `malloc` / `free` が含まれる (実行時間には含まれない)。
- `own.chs` の 4 ベンチ (n-body、fannkuch-redux、array-sort、spectral-norm) の命令数は変わらない (Haskell のフロントエンドは `case` を直接出す)。全ベンチの出力は変わらない (14 種)。
- Core の `Fields` の束縛で、箱なしのフィールドを遅延の `let` で束縛していた不具合 (`case (K (prim ..)) of K u -> ..` の形でだけ起きる) もここで直した。
- 変異 5 件はテストで全部検出した。
