# BUG-0047: Core Lint が、本体の発散するラムダを含む式を発散とみなし、その枝の線形性を検査しない

- 状態: todo
- 重大度: minor
- 発見箇所: FEAT-0040 のレビュー (`catch` の閉包の発散で見つかった同じ穴の一般の形)
- 関連: FEAT-0030, FEAT-0040

## 現象

`Expr::Lam` の使用 (`Usage`) は本体の `bottom` (発散) をそのまま持つ。ラムダの値を作ることは発散しないのに、本体が `panic` や `raise` で
終わるラムダを引数に渡す式・束縛する式が発散とみなされ、`case` の枝の合流 (`Usage::join`) で除かれる。その枝では線形の変数を使わなくても、
2 回使っても Lint が通る。

```lisp
(def bad (-> Int# (-> 1 Handle Int#)) (lam ((k Int#) (h 1 Handle))
  (case k (s Int#) Int#
    (0 (close h))
    (_ (apply_never (lam ((x Int#)) (panic "no" Int#)))))))
```

`k ≠ 0` の枝は `h` を使わないが、`(apply_never …)` が底とみなされるので受け付けられる。

## 再現手順

上のプログラムを `tests/core/fail/` に置いて `cargo test --test core` (Lint が受け付けてしまう)。

## 期待される動作

ラムダの値は発散しない。`Expr::Lam` の使用の `bottom` を偽にし、`h` が枝で違う使われ方をすると報告する
(FEAT-0040 で `catch` の閉包について同じことをした)。

## 原因

`lint.rs` の `Expr::Lam` が本体の使用をそのまま返す (`bottom` を含めて)。

## 修正

- [ ] 回帰テストを追加
- [ ] 修正

## メモ

- 既存のフロントエンドの出力がこの穴に頼っていないかを、全テストで確かめる。
