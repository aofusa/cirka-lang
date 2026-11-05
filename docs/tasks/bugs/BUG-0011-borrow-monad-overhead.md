# BUG-0011: Borrow モナドのループが線形な受け渡しより約 130 倍遅い

- 状態: todo
- 重大度: major
- 発見箇所: FEAT-0043 (`bench/n-body/own.chs` の借用版)
- 関連: FEAT-0124, FEAT-0123

## 現象

n-body の 1 ステップ (5 体、約 100 回の読み書き) を `borrowMut` と `Borrow` モナド
(`U.index (asRef m)` / `U.writeAt m`) で書くと、同じ処理を `U.read` / `U.write` で配列を線形に受け渡して書いた版より
約 130 倍遅い (20 万ステップで 28.6 s 対 0.22 s、`achc build -O2`)。結果は一致する。

## 再現手順

借用版のソースは `docs/artifacts/bench-0043/n-body-borrow.chs` (git 管理外。下の形のループ):

```haskell
advance :: Double -> MutRef r Bodies -> Borrow r ()
advance dt m = pairs 0 1 >> move 0
  where
    get k = U.index (asRef m) k
    add k d = get k >>= \v -> U.writeAt m k (v + d)
    ...
steps n b = case borrowMut b (advance 0.01) of (b', ()) -> steps (n - 1) b'
```

```sh
achc build -O2 -o nb n-body-borrow.chs && time ./nb 200000
```

## 期待される動作

借用を通した読み書きは、線形に受け渡す版と同程度 (Rust の `&mut [T]` 相当) の速さで動く。

## 原因

未調査。候補: `Borrow` モナドの `>>=` が IO 相当のクロージャを毎回確保する、`UElem` の辞書渡しが特殊化されず
`index` / `writeAt` が未知の関数呼び出しになる、ループの局所関数が worker/wrapper の対象外 (FEAT-0056)。

## 修正

- [ ] 回帰テストを追加 (確保回数か時間の上限)
- [ ] 修正

## メモ
