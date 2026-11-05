# BUG-0003: 捕捉数の異なる同一関数のクロージャが入口スタブを共有する

- 状態: done
- 重大度: critical
- 発見箇所: FEAT-0110 (`tests/achc/run/h1/map.hs` の `foldl (-) 0 xs`。クラス版 Prelude で `Foldable []` の `foldl` が `cirkaFoldlList` そのものになった)
- 関連: FEAT-0020 (LLVM バックエンド)

## 現象

ネイティブ実行だけがセグメンテーション違反 (debug ランタイムでは `dup: null object`) になる。MIR インタプリタは正しい結果を出す。

## 再現手順

```sh
cirka run tests/core/run/closure_shapes.core   # 修正前: -42 5 / 期待: 42 5
```

## 期待される動作

インタプリタと同じく `42 5` を出力する。

## 原因

LLVM バックエンドはクロージャの入口スタブを関数名だけで `f$clo` と名付けていた。
同じ関数 `f` を、値として使うクロージャ (捕捉 0 個) と部分適用 `f x y` (捕捉 2 個) の両方で作ると、
`clo_stubs` (関数名 → 捕捉数の表) が後から登録した捕捉数で上書きされ、スタブは一つしか生成されない。
捕捉 0 個のクロージャが捕捉 2 個用のスタブで呼ばれ、存在しない捕捉値を環境から読んでいた。

## 修正

- [x] 回帰テストを追加 (`tests/core/run/closure_shapes.core`)
- [x] スタブを (関数, 捕捉数) ごとに生成し、捕捉数 n > 0 のスタブを `f$clo{n}` と名付ける (`cirka-codegen-llvm`)

## メモ
