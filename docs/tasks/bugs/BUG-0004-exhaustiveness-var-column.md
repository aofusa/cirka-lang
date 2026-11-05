# BUG-0004: 変数を含む列の網羅性検査が誤って W0610 を出す

- 状態: done
- 重大度: minor
- 発見箇所: FEAT-0110 (`Semigroup (Maybe a)` のインスタンス)
- 関連: FEAT-0104

## 現象

次の定義は網羅的なのに `warning[W0610]: non-exhaustive patterns` が出る。

```haskell
f :: Maybe a -> Maybe a -> Int
f Nothing b = 0
f a Nothing = 1
f (Just a) (Just b) = 2
```

## 再現手順

```sh
achc check bug.hs   # 上の定義を含むファイル
```

## 期待される動作

警告を出さない (GHC も出さない)。

## 原因

W0610 を、生成した決定木に失敗の経路が残るかどうかで判定していた。第 1 列が変数の行へ落ちた後の決定木は
「第 1 列は `Just` である」ことを知らないので、もう一度 `Nothing` を調べて失敗する経路 (実際には到達しない) を作る。

## 修正

- [x] 回帰テストを追加 (`tests/achc/desugar/warnings.hs` の `both`、失敗しうるガードの `guarded`)
- [x] 修正: W0610 は Maranget の有用性 (usefulness) アルゴリズムで判定する (`achc-desugar/src/exhaust.rs`)。
  `otherwise`/`True` 以外のガードを持つ等式は網羅に数えない。決定木に残る到達しない失敗経路は従来どおり panic にする。

## メモ

`Cirka.Base` では最初の列にも `Just a` と書いて回避している。
